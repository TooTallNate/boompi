//! BlueZ pairing agent (`org.bluez.Agent1`), capability `NoInputNoOutput`.
//!
//! Replaces v1's *unconditional* auto-accept (`bt-agent`): pairing consent
//! is surfaced to the panel/web UI (`Pairing` state) and blocks until the
//! user approves, rejects, or a 30 s timeout fires.
//!
//! Capability is `NoInputNoOutput` (JustWorks), not `DisplayYesNo`: the
//! boombox's USB dongle is a counterfeit CSR (0a12:0001) whose SSP
//! implementation cannot complete MITM numeric comparison — advertising
//! display capability makes iOS request it and pairing dies at the radio
//! layer before bluetoothd says a word (v1 worked because JustWorks never
//! asks for it). JustWorks consent arrives via `RequestAuthorization`
//! (no passkey); `RequestConfirmation` stays wired for future boxes with
//! real SSP silicon.
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
    /// Recent consent decisions per device path. One phone connection
    /// authorizes several UUIDs (A2DP, AVRCP, ...) in a burst — ask once,
    /// reuse the answer for the rest of the burst.
    recent: Mutex<std::collections::HashMap<String, (std::time::Instant, bool)>>,
    /// Serializes user prompts: burst UUID authorizations arrive
    /// concurrently, and parallel `ask_user` calls would stomp each
    /// other's decision channel (instant implicit rejects). Waiters queue
    /// here, then read the first answer from `recent`.
    prompt: tokio::sync::Mutex<()>,
}

impl Agent {
    /// Surface a pairing request and block on the user's decision
    /// (30 s timeout → reject). `passkey` is shown when present (numeric
    /// comparison); JustWorks consent has none.
    async fn ask_user(&self, device: &OwnedObjectPath, passkey: Option<u32>) -> bool {
        let name = device_alias(&self.conn, device).await;
        tracing::info!(device = ?name, ?passkey, "pairing consent requested");
        let (tx, rx) = oneshot::channel();
        *self.decision.lock().unwrap() = Some(tx);
        set_pairing(
            &self.app,
            Pairing {
                state: PairingState::Confirm,
                device_name: name,
                passkey,
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
        tracing::info!(confirmed, "pairing consent resolved");
        confirmed
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    /// SSP numeric comparison — only reachable on adapters with real
    /// display-capable SSP (not the counterfeit-CSR dongle).
    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), AgentError> {
        if self.ask_user(&device, Some(passkey)).await {
            Ok(())
        } else {
            Err(AgentError::Rejected("pairing rejected on device".into()))
        }
    }

    /// JustWorks pairing consent — in practice bluetoothd auto-accepts
    /// JustWorks with a NoInputNoOutput agent and never calls this, but
    /// keep it wired for stacks/paths that do.
    async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        if self.ask_user(&device, None).await {
            Ok(())
        } else {
            Err(AgentError::Rejected("pairing rejected on device".into()))
        }
    }

    /// Profile (A2DP/AVRCP/...) authorization. Trusted devices never reach
    /// here — this fires on a device's *first* connection after JustWorks
    /// pairing, which (with the counterfeit-CSR dongle silently
    /// auto-pairing) is the one consent point BlueZ actually gives us.
    /// Accept → mark Trusted (never asks again); reject → unpair.
    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        uuid: String,
    ) -> Result<(), AgentError> {
        // Serialize prompts, then check the burst cache: whoever loses the
        // race waits here and reuses the winner's answer.
        let _prompting = self.prompt.lock().await;
        if let Some((when, allowed)) = self.recent.lock().unwrap().get(device.as_str()).copied() {
            if when.elapsed() < Duration::from_secs(60) {
                return if allowed {
                    Ok(())
                } else {
                    Err(AgentError::Rejected("connection rejected".into()))
                };
            }
        }
        tracing::info!(device = %device.as_str(), %uuid, "first-connect authorization");
        let allowed = self.ask_user(&device, None).await;
        self.recent
            .lock()
            .unwrap()
            .insert(device.as_str().to_string(), (std::time::Instant::now(), allowed));

        let conn = self.conn.clone();
        let dev = device.clone();
        if allowed {
            // Persist: trusted devices skip AuthorizeService entirely.
            tokio::spawn(async move {
                let result = async {
                    zbus::Proxy::new(&conn, "org.bluez", dev.clone(), "org.bluez.Device1")
                        .await?
                        .set_property("Trusted", true)
                        .await
                }
                .await;
                if let Err(err) = result {
                    tracing::warn!(%err, "failed to mark device trusted");
                }
            });
            Ok(())
        } else {
            // A rejected first connect = user said no to this device:
            // undo the (silently auto-accepted) pairing too.
            tokio::spawn(async move {
                let result: anyhow::Result<()> = async {
                    let adapter = dev
                        .as_str()
                        .rsplit_once("/dev_")
                        .map(|(a, _)| a.to_string())
                        .unwrap_or_else(|| "/org/bluez/hci0".into());
                    let proxy = zbus::Proxy::new(&conn, "org.bluez", adapter, "org.bluez.Adapter1")
                        .await?;
                    proxy
                        .call_method("RemoveDevice", &(zbus::zvariant::ObjectPath::from(dev)))
                        .await?;
                    Ok(())
                }
                .await;
                if let Err(err) = result {
                    tracing::warn!(%err, "failed to remove rejected device");
                }
            });
            Err(AgentError::Rejected("connection rejected".into()))
        }
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
        recent: Mutex::new(std::collections::HashMap::new()),
        prompt: tokio::sync::Mutex::new(()),
    };
    // `at` returns false when the interface is already served (restart) —
    // the existing instance holds stale channel refs, so replace it.
    let server = conn.object_server();
    let _ = server.remove::<Agent, _>(&path).await;
    server.at(&path, agent).await?;

    let manager = AgentManager1Proxy::new(conn).await?;
    let _ = manager.unregister_agent(&path).await; // stale registration
    manager.register_agent(&path, "NoInputNoOutput").await?;
    manager.request_default_agent(&path).await?;
    tracing::info!("pairing agent registered (NoInputNoOutput / JustWorks)");
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
