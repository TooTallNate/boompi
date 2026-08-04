//! BlueZ pairing agent (`org.bluez.Agent1`), capability `DisplayYesNo`.
//!
//! Replaces v1's auto-accept `bt-agent -c NoInputNoOutput`: modern phones
//! use SSP numeric comparison, so BlueZ calls `RequestConfirmation` with a
//! 6-digit passkey; we surface it to the panel/web UI (`Pairing` state) and
//! block until the user confirms, rejects, or a 30 s timeout fires.
//!
//! The decision arrives through [`DecisionSlot`] — the bluetooth task owns
//! the other end and resolves it from `ClientMessage::Pairing` commands.

#![cfg(target_os = "linux")]

use crate::state::SharedApp;
use boompi_proto::{Pairing, PairingState, ServerMessage};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use zbus::zvariant::OwnedObjectPath;

pub const AGENT_PATH: &str = "/com/boompi/agent";

/// Slot for the pending pairing decision. `Some` while a confirmation is
/// on screen; the bluetooth task takes the sender to resolve it.
pub type DecisionSlot = Arc<Mutex<Option<oneshot::Sender<bool>>>>;

/// BlueZ agent errors — the names (org.bluez.Error.*) are meaningful to
/// bluetoothd, so they must survive the D-Bus round trip exactly.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}

#[zbus::proxy(
    interface = "org.bluez.AgentManager1",
    default_service = "org.bluez",
    default_path = "/org/bluez"
)]
trait AgentManager1 {
    fn register_agent(
        &self,
        agent: &zbus::zvariant::ObjectPath<'_>,
        capability: &str,
    ) -> zbus::Result<()>;
    fn request_default_agent(&self, agent: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
    fn unregister_agent(&self, agent: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

pub struct Agent {
    app: SharedApp,
    conn: zbus::Connection,
    decision: DecisionSlot,
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    /// SSP numeric comparison — the main path for phones.
    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), AgentError> {
        let name = device_alias(&self.conn, &device).await;
        tracing::info!(device = ?name, passkey, "pairing confirmation requested");
        let (tx, rx) = oneshot::channel();
        *self.decision.lock().unwrap() = Some(tx);
        set_pairing(
            &self.app,
            Pairing {
                state: PairingState::Confirm,
                device_name: name,
                passkey: Some(passkey),
            },
        )
        .await;

        let confirmed = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false);
        self.decision.lock().unwrap().take();
        // Back to plain-discoverable; a successful pair flips to Idle when
        // the Paired property change lands in the bluetooth task.
        set_pairing(
            &self.app,
            Pairing {
                state: PairingState::Discoverable,
                ..Pairing::default()
            },
        )
        .await;

        if confirmed {
            tracing::info!("pairing confirmed");
            Ok(())
        } else {
            tracing::info!("pairing rejected (or timed out)");
            Err(AgentError::Rejected("pairing rejected on device".into()))
        }
    }

    /// Incoming connection from an already-paired device.
    async fn request_authorization(&self, _device: OwnedObjectPath) -> Result<(), AgentError> {
        Ok(())
    }

    /// Profile (A2DP/AVRCP/...) authorization for paired devices.
    async fn authorize_service(
        &self,
        _device: OwnedObjectPath,
        _uuid: String,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    // Legacy PIN flows — DisplayYesNo shouldn't receive these, but answer
    // deterministically if an odd stack probes them.
    async fn request_pin_code(&self, _device: OwnedObjectPath) -> Result<String, AgentError> {
        Err(AgentError::Rejected("PIN pairing not supported".into()))
    }

    async fn request_passkey(&self, _device: OwnedObjectPath) -> Result<u32, AgentError> {
        Err(AgentError::Rejected("passkey entry not supported".into()))
    }

    fn display_passkey(&self, _device: OwnedObjectPath, _passkey: u32, _entered: u16) {}

    fn display_pin_code(&self, _device: OwnedObjectPath, _pincode: String) {}

    /// BlueZ aborted an in-flight request (remote side cancelled).
    async fn cancel(&self) {
        tracing::info!("pairing request cancelled by BlueZ");
        if let Some(tx) = self.decision.lock().unwrap().take() {
            let _ = tx.send(false);
        }
    }

    fn release(&self) {
        tracing::debug!("pairing agent released");
    }
}

/// Register (or re-register after a task restart) as the default agent.
pub async fn register(
    conn: &zbus::Connection,
    app: SharedApp,
    decision: DecisionSlot,
) -> anyhow::Result<()> {
    let path = zbus::zvariant::ObjectPath::try_from(AGENT_PATH)?;
    let agent = Agent {
        app,
        conn: conn.clone(),
        decision,
    };
    // `at` returns false when the interface is already served (restart) —
    // the existing instance holds stale channel refs, so replace it.
    let server = conn.object_server();
    let _ = server.remove::<Agent, _>(&path).await;
    server.at(&path, agent).await?;

    let manager = AgentManager1Proxy::new(conn).await?;
    let _ = manager.unregister_agent(&path).await; // stale registration
    manager.register_agent(&path, "DisplayYesNo").await?;
    manager.request_default_agent(&path).await?;
    tracing::info!("pairing agent registered (DisplayYesNo)");
    Ok(())
}

/// Update shared pairing state + notify clients.
pub async fn set_pairing(app: &SharedApp, pairing: Pairing) {
    let mut s = app.shared.write().await;
    if s.pairing != pairing {
        s.pairing = pairing.clone();
        drop(s);
        app.broadcast(ServerMessage::Pairing(pairing));
    }
}

async fn device_alias(conn: &zbus::Connection, path: &OwnedObjectPath) -> Option<String> {
    let proxy = zbus::Proxy::new(conn, "org.bluez", path.clone(), "org.bluez.Device1")
        .await
        .ok()?;
    proxy.get_property::<String>("Alias").await.ok()
}
