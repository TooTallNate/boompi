//! BLE GATT control bridge: the WebSocket protocol over Bluetooth LE.
//!
//! Registers a GATT application + LE advertisement with BlueZ (zbus, same
//! stack as `bluetooth.rs`) so phones can control the speaker with **no
//! shared IP network at all** - the door-opener for a native iOS app
//! (CoreBluetooth) and Web Bluetooth on Android Chrome. See docs/BLE.md
//! for the full contract; the UUIDs + chunk framing live in
//! [`boompi_proto::ble`] so client ports have one source of truth.
//!
//! Three characteristics under one primary service:
//!
//! - `control` (write): chunked JSON [`ClientMessage`] - same dispatch as
//!   the WebSocket.
//! - `events` (notify): chunked JSON [`ServerMessage`] deltas, i.e. the
//!   daemon's broadcast stream. Subscribing greets the client with
//!   `hello` + a full `state` snapshot, mirroring a WebSocket connect.
//! - `state` (read): full JSON [`State`] snapshot via GATT long-read.
//!
//! Deliberately NOT bridged: binary frames (visualizer bars at 30 fps and
//! artwork bytes would swamp a ~5-50 KB/s LE link). Artwork stays on
//! `GET /art/{id}` for clients that also have an IP path (e.g. joined the
//! speaker's hotspot).
//!
//! Security model: same as the LAN HTTP/WebSocket API - the control
//! channel is open (JustWorks, no bond required), consistent with the
//! open onboarding/camping hotspot. A2DP pairing still goes through the
//! explicit pairing window in `bt_agent.rs`; this service neither needs
//! nor triggers it.

#![cfg(target_os = "linux")]

use crate::state::{Outbound, SharedApp};
use boompi_proto::{ble, ClientMessage, ServerMessage};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zbus::fdo::ObjectManagerProxy;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

const APP_PATH: &str = "/com/boompi/ble";
const SERVICE_PATH: &str = "/com/boompi/ble/service0";
const CONTROL_PATH: &str = "/com/boompi/ble/service0/control";
const EVENTS_PATH: &str = "/com/boompi/ble/service0/events";
const STATE_PATH: &str = "/com/boompi/ble/service0/state";
const ADV_PATH: &str = "/com/boompi/ble/advertisement0";

#[zbus::proxy(interface = "org.bluez.GattManager1", default_service = "org.bluez")]
trait GattManager1 {
    fn register_application(
        &self,
        application: &ObjectPath<'_>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;
    fn unregister_application(&self, application: &ObjectPath<'_>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.bluez.LEAdvertisingManager1",
    default_service = "org.bluez"
)]
trait LEAdvertisingManager1 {
    fn register_advertisement(
        &self,
        advertisement: &ObjectPath<'_>,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;
    fn unregister_advertisement(&self, advertisement: &ObjectPath<'_>) -> zbus::Result<()>;
}

/// BlueZ GATT errors - the org.bluez.Error.* names are meaningful to
/// bluetoothd and map to ATT error responses on the radio.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum GattError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    NotSupported(String),
}

/// Per-run state shared by the characteristics: the largest ATT MTU seen
/// in read/write options (0 = unknown → conservative default). Single
/// phone at a time in practice; with several, the smallest MTU client
/// still reassembles fine (chunk framing is MTU-agnostic).
#[derive(Default)]
struct BleShared {
    mtu: AtomicUsize,
    /// Serializes whole-message notification sends: the greeting task
    /// and the broadcast pump both call [`notify_events`], and chunks
    /// from two messages must never interleave (the reassembler would
    /// drop both).
    send_lock: tokio::sync::Mutex<()>,
}

impl BleShared {
    fn note_mtu(&self, options: &HashMap<String, OwnedValue>) {
        if let Some(mtu) = options.get("mtu").and_then(|v| u16::try_from(v).ok()) {
            self.mtu.store(mtu as usize, Ordering::Relaxed);
        }
    }

    /// Max chunk (header + payload) for notifications: ATT notification
    /// payload is MTU - 3.
    fn chunk_size(&self) -> usize {
        match self.mtu.load(Ordering::Relaxed) {
            0 => ble::DEFAULT_CHUNK,
            mtu => (mtu.saturating_sub(3)).clamp(2, 512),
        }
    }
}

struct GattService;

#[zbus::interface(name = "org.bluez.GattService1")]
impl GattService {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        ble::SERVICE_UUID.into()
    }

    #[zbus(property)]
    fn primary(&self) -> bool {
        true
    }
}

struct ControlChar {
    app: SharedApp,
    shared: Arc<BleShared>,
    reassembler: ble::Reassembler,
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl ControlChar {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        ble::CONTROL_CHAR_UUID.into()
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        ObjectPath::from_static_str_unchecked(SERVICE_PATH).into()
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<String> {
        vec!["write".into(), "write-without-response".into()]
    }

    async fn write_value(
        &mut self,
        value: Vec<u8>,
        options: HashMap<String, OwnedValue>,
    ) -> Result<(), GattError> {
        self.shared.note_mtu(&options);
        let Some(message) = self.reassembler.push(&value) else {
            return Ok(()); // mid-message chunk (or resync drop)
        };
        match serde_json::from_slice::<ClientMessage>(&message) {
            // BatteryFastPoll is refcounted per WebSocket connection and
            // released on disconnect; BLE has no such lifecycle hook, so
            // it is not supported on this transport (see docs/BLE.md).
            Ok(ClientMessage::BatteryFastPoll { .. }) => {}
            Ok(msg) => self.app.handle_client_message(msg).await,
            Err(err) => {
                tracing::warn!(%err, "unparseable BLE client message");
                return Err(GattError::Failed(format!("bad message: {err}")));
            }
        }
        Ok(())
    }
}

struct EventsChar {
    app: SharedApp,
    conn: zbus::Connection,
    shared: Arc<BleShared>,
    /// Last chunk pushed; notifications are PropertiesChanged on this.
    value: Vec<u8>,
    notifying: bool,
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl EventsChar {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        ble::EVENTS_CHAR_UUID.into()
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        ObjectPath::from_static_str_unchecked(SERVICE_PATH).into()
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<String> {
        vec!["notify".into()]
    }

    #[zbus(property)]
    fn value(&self) -> Vec<u8> {
        self.value.clone()
    }

    async fn start_notify(&mut self) -> Result<(), GattError> {
        self.notifying = true;
        tracing::info!("BLE client subscribed to events");
        // Greet like a WebSocket connect: hello + full state snapshot.
        // Spawned so it runs after this method releases the interface
        // lock (notify_events needs it). With multiple subscribed
        // centrals the greeting reaches all of them - harmless, the
        // snapshot is idempotent.
        let app = self.app.clone();
        let conn = self.conn.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let hello = ServerMessage::Hello(crate::server::hello(&app).await);
            let state = ServerMessage::State(app.snapshot().await);
            for msg in [hello, state] {
                match serde_json::to_vec(&msg) {
                    Ok(json) => {
                        if let Err(err) = notify_events(&conn, &json).await {
                            tracing::debug!(%err, "BLE greeting notify failed");
                            return;
                        }
                    }
                    Err(err) => tracing::error!(%err, "BLE greeting serialize failed"),
                }
            }
        });
        Ok(())
    }

    async fn stop_notify(&mut self) -> Result<(), GattError> {
        self.notifying = false;
        tracing::info!("BLE client unsubscribed from events");
        Ok(())
    }

    async fn read_value(
        &self,
        _options: HashMap<String, OwnedValue>,
    ) -> Result<Vec<u8>, GattError> {
        Err(GattError::NotSupported("subscribe instead".into()))
    }
}

struct StateChar {
    app: SharedApp,
    shared: Arc<BleShared>,
    /// Snapshot taken at offset 0; offset>0 long-read continuations
    /// slice it so a multi-request read never tears across updates.
    cache: Vec<u8>,
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl StateChar {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        ble::STATE_CHAR_UUID.into()
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        ObjectPath::from_static_str_unchecked(SERVICE_PATH).into()
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<String> {
        vec!["read".into()]
    }

    async fn read_value(
        &mut self,
        options: HashMap<String, OwnedValue>,
    ) -> Result<Vec<u8>, GattError> {
        self.shared.note_mtu(&options);
        let offset = options
            .get("offset")
            .and_then(|v| u16::try_from(v).ok())
            .unwrap_or(0) as usize;
        if offset == 0 {
            self.cache = serde_json::to_vec(&self.app.snapshot().await)
                .map_err(|e| GattError::Failed(e.to_string()))?;
        }
        Ok(self.cache.get(offset..).unwrap_or_default().to_vec())
    }
}

/// LE advertisement carrying the service UUID + speaker name so scanning
/// apps can filter and label.
struct Advertisement {
    local_name: String,
}

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "peripheral".into()
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn service_uuids(&self) -> Vec<String> {
        vec![ble::SERVICE_UUID.into()]
    }

    #[zbus(property)]
    fn local_name(&self) -> String {
        self.local_name.clone()
    }

    #[zbus(property)]
    fn discoverable(&self) -> bool {
        true
    }

    fn release(&self) {
        tracing::debug!("BLE advertisement released by BlueZ");
    }
}

/// Push one protocol message to subscribed BLE clients, chunked to the
/// negotiated MTU. No-op while nobody is subscribed. Whole messages are
/// sent atomically (send_lock): concurrent callers (greeting task vs.
/// the broadcast pump) must not interleave their chunk sequences.
async fn notify_events(conn: &zbus::Connection, payload: &[u8]) -> anyhow::Result<()> {
    let iface = conn
        .object_server()
        .interface::<_, EventsChar>(EVENTS_PATH)
        .await?;
    let shared = {
        let guard = iface.get().await;
        if !guard.notifying {
            return Ok(());
        }
        guard.shared.clone()
    };
    let _sending = shared.send_lock.lock().await;
    let chunk_size = shared.chunk_size();
    for chunk in ble::chunk_message(payload, chunk_size) {
        iface.get_mut().await.value = chunk;
        iface
            .get()
            .await
            .value_changed(iface.signal_emitter())
            .await?;
    }
    Ok(())
}

/// Locate the (first) adapter exposing a GATT server.
async fn find_gatt_adapter(conn: &zbus::Connection) -> anyhow::Result<Option<OwnedObjectPath>> {
    let om = ObjectManagerProxy::builder(conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    for (path, interfaces) in om.get_managed_objects().await? {
        if interfaces.contains_key("org.bluez.GattManager1") {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub fn spawn(app: SharedApp) {
    tokio::spawn(async move {
        loop {
            if let Err(err) = run(&app).await {
                tracing::warn!(%err, "BLE GATT bridge stopped; retrying in 10s");
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

async fn run(app: &SharedApp) -> anyhow::Result<()> {
    // Own connection (like every hardware module): the object tree +
    // registrations die with it, so a task restart starts clean.
    let conn = zbus::Connection::system().await?;
    let shared = Arc::new(BleShared::default());

    let server = conn.object_server();
    server.at(APP_PATH, zbus::fdo::ObjectManager).await?;
    server.at(SERVICE_PATH, GattService).await?;
    server
        .at(
            CONTROL_PATH,
            ControlChar {
                app: app.clone(),
                shared: shared.clone(),
                reassembler: ble::Reassembler::default(),
            },
        )
        .await?;
    server
        .at(
            EVENTS_PATH,
            EventsChar {
                app: app.clone(),
                conn: conn.clone(),
                shared: shared.clone(),
                value: Vec::new(),
                notifying: false,
            },
        )
        .await?;
    server
        .at(
            STATE_PATH,
            StateChar {
                app: app.clone(),
                shared,
                cache: Vec::new(),
            },
        )
        .await?;
    server
        .at(
            ADV_PATH,
            Advertisement {
                local_name: app.speaker_name().await,
            },
        )
        .await?;

    // Wait for a GATT-capable adapter (dongle may be unplugged, or
    // bluetoothd still starting). A bluetoothd restart later is caught
    // by the NameOwnerChanged watch below.
    let adapter = loop {
        match find_gatt_adapter(&conn).await {
            Ok(Some(path)) => break path,
            Ok(None) => tracing::debug!("no GATT-capable adapter yet; waiting"),
            Err(err) => tracing::debug!(%err, "bluez not reachable yet; waiting"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    };

    let app_path = ObjectPath::try_from(APP_PATH)?;
    let gatt = GattManager1Proxy::builder(&conn)
        .path(adapter.clone())?
        .build()
        .await?;
    gatt.register_application(&app_path, HashMap::new()).await?;
    tracing::info!(adapter = %adapter, "BLE GATT control service registered");

    // Advertising is best-effort: some dongles register the app fine but
    // reject LE advertising - the service still works for clients that
    // already know the address.
    let adv_path = ObjectPath::try_from(ADV_PATH)?;
    let advm = LEAdvertisingManager1Proxy::builder(&conn)
        .path(adapter.clone())?
        .build()
        .await?;
    match advm.register_advertisement(&adv_path, HashMap::new()).await {
        Ok(()) => tracing::info!("BLE advertisement registered"),
        Err(err) => tracing::warn!(%err, "BLE advertising unavailable (GATT still active)"),
    }

    // Restart from scratch when bluetoothd bounces (registrations void).
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    let mut owner_stream = dbus.receive_name_owner_changed().await?;

    // Speaker renames re-register the advertisement under the new name.
    let mut cfg_watch = app.subscribe_cfg();
    cfg_watch.mark_unchanged();

    let mut rx = app.tx.subscribe();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Ok(Outbound::Message(json)) => {
                    if let Err(err) = notify_events(&conn, json.as_bytes()).await {
                        tracing::debug!(%err, "BLE notify failed");
                    }
                }
                // Binary frames (visualizer/artwork) stay off BLE.
                Ok(Outbound::Frame(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "BLE bridge lagged behind broadcasts");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    anyhow::bail!("broadcast channel closed")
                }
            },
            _ = cfg_watch.changed() => {
                let name = app.speaker_name().await;
                if let Ok(iface) = conn
                    .object_server()
                    .interface::<_, Advertisement>(ADV_PATH)
                    .await
                {
                    iface.get_mut().await.local_name = name;
                    let _ = advm.unregister_advertisement(&adv_path).await;
                    if let Err(err) = advm
                        .register_advertisement(&adv_path, HashMap::new())
                        .await
                    {
                        tracing::warn!(%err, "BLE advertisement re-registration failed");
                    }
                }
            }
            Some(change) = owner_stream.next() => {
                if let Ok(args) = change.args() {
                    if args.name.as_str() == "org.bluez" {
                        anyhow::bail!("bluetoothd restarted; re-registering GATT app");
                    }
                }
            }
        }
    }
}
