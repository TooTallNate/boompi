//! Bluetooth audio source: BlueZ over D-Bus (zbus).
//!
//! Mirrors the v1 Node.js integration, but handles devices appearing at
//! runtime (v1 TODO) by driving everything from `ObjectManager` +
//! `PropertiesChanged` on the `org.bluez` service:
//!
//! - `org.bluez.Device1` — connect/disconnect + device alias
//! - `org.bluez.MediaPlayer1` — track metadata, playback status, transport
//! - `org.bluez.MediaTransport1` — AVRCP absolute volume (0–127)
//!
//! Pairing agent (`Agent1`) and cover art (obexd/BIP) land in Phase 3.

#![cfg(target_os = "linux")]

use crate::state::{now_ms, BtCommand, SharedApp, SourceCommand};
use boompi_proto::{
    BtDevice, BtDeviceAction, Pairing, PairingAction, PairingState, PlaybackStatus, ServerMessage,
    SourceInfo, SourceKind, Track,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::fdo::ObjectManagerProxy;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, MessageStream};

#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
trait Adapter1 {
    /// Controller alias — the name phones see when pairing/connecting.
    #[zbus(property)]
    fn set_alias(&self, alias: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_discoverable(&self, discoverable: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_pairable(&self, pairable: bool) -> zbus::Result<()>;
    fn remove_device(&self, device: &ObjectPath<'_>) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_trusted(&self, trusted: bool) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.MediaPlayer1", default_service = "org.bluez")]
trait MediaPlayer1 {
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn track(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<u32>;
    /// BIP OBEX PSM for AVRCP cover art. `[experimental]` — present only
    /// with `Experimental = true` and a phone that supports cover art.
    #[zbus(property)]
    fn obex_port(&self) -> zbus::Result<u16>;
}

#[zbus::proxy(interface = "org.bluez.MediaTransport1", default_service = "org.bluez")]
trait MediaTransport1 {
    #[zbus(property)]
    fn volume(&self) -> zbus::Result<u16>;
    #[zbus(property)]
    fn set_volume(&self, volume: u16) -> zbus::Result<()>;
}

/// AVRCP absolute volume is 0–127.
const AVRCP_MAX: f32 = 127.0;

/// Mutable view of the currently connected phone/player.
#[derive(Default)]
struct Session {
    /// Local controller (survives phone disconnects).
    adapter_path: Option<OwnedObjectPath>,
    device_path: Option<String>,
    device_alias: Option<String>,
    player_path: Option<OwnedObjectPath>,
    transport_path: Option<OwnedObjectPath>,
    // Last-known track state (BlueZ sends partial updates).
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: Option<u32>,
    position_ms: Option<u32>,
    status: PlaybackStatus,
    // Cover art (AVRCP 1.6 / BIP).
    obex_port: Option<u16>,
    img_handle: Option<String>,
    art_requested_for: Option<String>,
}

impl Session {
    fn to_track(&self, resolved: &crate::artwork::ResolvedArt) -> Track {
        let artwork_id = self
            .img_handle
            .as_ref()
            .and_then(|h| resolved.lock().unwrap().get(h).cloned());
        Track {
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration_ms: self.duration_ms,
            position_ms: self.position_ms,
            status: self.status,
            artwork_id,
            updated_at: now_ms(),
        }
    }

    /// Device address in colon form, from the BlueZ object path.
    fn address(&self) -> Option<String> {
        let path = self.device_path.as_ref()?;
        let mac = path.rsplit("/dev_").next()?;
        Some(mac.replace('_', ":"))
    }
}

/// Shared handles for the event handlers.
struct Ctx {
    app: SharedApp,
    conn: zbus::Connection,
    resolved: crate::artwork::ResolvedArt,
    art_tx: mpsc::UnboundedSender<crate::artwork::ArtRequest>,
}

pub fn spawn(app: SharedApp) {
    let (tx, rx) = mpsc::unbounded_channel();
    app.register_source(SourceKind::Bluetooth, tx);
    let (bt_tx, bt_rx) = mpsc::unbounded_channel();
    app.register_bt_ctl(bt_tx);
    let resolved: crate::artwork::ResolvedArt = Default::default();
    let art_tx = crate::artwork::spawn(app.clone(), resolved.clone());
    tokio::spawn(async move {
        let mut rx = rx;
        let mut bt_rx = bt_rx;
        loop {
            if let Err(err) = run(&app, &resolved, &art_tx, &mut rx, &mut bt_rx).await {
                tracing::error!(%err, "bluetooth source failed; retrying in 5s");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

async fn run(
    app: &SharedApp,
    resolved: &crate::artwork::ResolvedArt,
    art_tx: &mpsc::UnboundedSender<crate::artwork::ArtRequest>,
    cmds: &mut mpsc::UnboundedReceiver<SourceCommand>,
    bt_cmds: &mut mpsc::UnboundedReceiver<BtCommand>,
) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    tracing::info!("connected to system D-Bus, watching org.bluez");
    let ctx = Ctx {
        app: app.clone(),
        conn: conn.clone(),
        resolved: resolved.clone(),
        art_tx: art_tx.clone(),
    };

    // Pairing agent: decisions resolved via BtCommand::Pairing below.
    let decision: crate::bt_agent::DecisionSlot = Default::default();
    if let Err(err) = crate::bt_agent::register(&conn, app.clone(), decision.clone()).await {
        tracing::warn!(%err, "pairing agent registration failed (pairing UI unavailable)");
    }

    // All property changes under /org/bluez in one subscription (v1 did the
    // same): Device1.Connected, MediaPlayer1.*, MediaTransport1.Volume.
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.bluez")?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path_namespace("/org/bluez")?
        .build();
    let mut props_stream = MessageStream::for_match_rule(rule, &conn, Some(64)).await?;

    let om = ObjectManagerProxy::builder(&conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    let mut added_stream = om.receive_interfaces_added().await?;
    let mut removed_stream = om.receive_interfaces_removed().await?;

    // A bluetoothd restart silently voids our agent registration and
    // session state (signals keep flowing — the well-known name just
    // changes owners). Watch for it and restart this task from scratch.
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    let mut owner_stream = dbus.receive_name_owner_changed().await?;

    let mut session = Session::default();
    let mut cfg_watch = app.subscribe_cfg();
    cfg_watch.mark_unchanged();

    // Adopt whatever is already connected (e.g. phone paired+connected
    // before boompid started).
    for (path, interfaces) in om.get_managed_objects().await? {
        for (iface, props) in &interfaces {
            handle_interface_added(&ctx, &mut session, &path, iface.as_str(), props).await;
        }
    }
    // The name phones see: keep the controller alias in sync with config.
    apply_adapter_alias(&ctx, &session, &app.speaker_name().await).await;
    refresh_devices(&ctx).await;

    loop {
        tokio::select! {
            _ = cfg_watch.changed() => {
                apply_adapter_alias(&ctx, &session, &app.speaker_name().await).await;
            }
            cmd = bt_cmds.recv() => {
                let Some(cmd) = cmd else { anyhow::bail!("bt control channel closed") };
                handle_bt_command(&ctx, &session, &decision, cmd).await;
            }
            Some(change) = owner_stream.next() => {
                if let Ok(args) = change.args() {
                    if args.name.as_str() == "org.bluez" && args.new_owner.is_some() {
                        anyhow::bail!("bluetoothd restarted; re-initializing");
                    }
                }
            }
            msg = props_stream.next() => {
                let Some(Ok(msg)) = msg else { anyhow::bail!("D-Bus signal stream ended") };
                let Some(path) = msg.header().path().map(|p| p.to_string()) else { continue };
                let body = msg.body();
                let Ok((iface, changed, _invalidated)) = body
                    .deserialize::<(String, HashMap<String, Value>, Vec<String>)>() else { continue };
                handle_properties_changed(&ctx, &mut session, &path, &iface, &changed).await;
            }
            added = added_stream.next() => {
                let Some(added) = added else { anyhow::bail!("InterfacesAdded stream ended") };
                if let Ok(args) = added.args() {
                    let path = OwnedObjectPath::from(args.object_path.clone());
                    for (iface, props) in &args.interfaces_and_properties {
                        // Borrowed signal args → owned map for shared handling.
                        let props: HashMap<String, OwnedValue> = props
                            .iter()
                            .filter_map(|(k, v)| {
                                v.try_to_owned().ok().map(|ov| (k.to_string(), ov))
                            })
                            .collect();
                        handle_interface_added(&ctx, &mut session, &path, iface, &props)
                            .await;
                    }
                }
            }
            removed = removed_stream.next() => {
                let Some(removed) = removed else { anyhow::bail!("InterfacesRemoved stream ended") };
                if let Ok(args) = removed.args() {
                    let ifaces: Vec<String> = args.interfaces.iter().map(|i| i.to_string()).collect();
                    handle_interfaces_removed(&ctx, &mut session, args.object_path.as_str(), &ifaces).await;
                }
            }
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { anyhow::bail!("command channel closed") };
                if let Err(err) = handle_command(&ctx, &session, cmd).await {
                    tracing::warn!(%err, ?cmd, "source command failed");
                }
            }
        }
    }
}

async fn handle_interface_added(
    ctx: &Ctx,
    session: &mut Session,
    path: &OwnedObjectPath,
    iface: &str,
    props: &HashMap<String, OwnedValue>,
) {
    match iface {
        "org.bluez.Adapter1" => {
            session.adapter_path = Some(path.clone());
        }
        "org.bluez.Device1" => {
            refresh_devices(ctx).await;
        }
        "org.bluez.MediaPlayer1" => {
            tracing::info!(%path, "media player appeared");
            session.player_path = Some(path.clone());
            adopt_device_of(ctx, session, path.as_str()).await;
            // Seed initial state.
            if let Some(track) = props.get("Track").and_then(dict_from_value) {
                apply_track_dict(session, &track);
            }
            if let Some(status) = props.get("Status").and_then(str_from_value) {
                session.status = parse_status(&status);
            }
            if let Some(pos) = props.get("Position").and_then(u32_from_value) {
                session.position_ms = Some(pos);
            }
            // Cover art support? (experimental property; absent otherwise)
            session.obex_port = match props.get("ObexPort").and_then(u16_from_value) {
                Some(port) => Some(port),
                None => match MediaPlayer1Proxy::builder(&ctx.conn)
                    .path(path.clone())
                    .unwrap()
                    .build()
                    .await
                {
                    Ok(proxy) => proxy.obex_port().await.ok(),
                    Err(_) => None,
                },
            };
            if let Some(port) = session.obex_port {
                tracing::info!(port, "player supports AVRCP cover art");
                prime_art_session(ctx, session);
            }
            publish_track(ctx, session).await;
        }
        "org.bluez.MediaTransport1" => {
            tracing::info!(%path, "media transport appeared");
            session.transport_path = Some(path.clone());
            let volume = props.get("Volume").and_then(u16_from_value);
            let volume = match volume {
                Some(v) => Some(v),
                None => match MediaTransport1Proxy::builder(&ctx.conn)
                    .path(path.clone())
                    .unwrap()
                    .build()
                    .await
                {
                    Ok(proxy) => proxy.volume().await.ok(),
                    Err(_) => None,
                },
            };
            if let Some(v) = volume {
                apply_phone_volume(&ctx.app, v).await;
            }
        }
        _ => {}
    }
}

async fn handle_interfaces_removed(
    ctx: &Ctx,
    session: &mut Session,
    path: &str,
    interfaces: &[String],
) {
    for iface in interfaces {
        match iface.as_str() {
            "org.bluez.MediaPlayer1"
                if session
                    .player_path
                    .as_ref()
                    .is_some_and(|p| p.as_str() == path) =>
            {
                tracing::info!(%path, "media player removed");
                session.player_path = None;
                clear_session(ctx, session).await;
            }
            "org.bluez.MediaTransport1"
                if session
                    .transport_path
                    .as_ref()
                    .is_some_and(|p| p.as_str() == path) =>
            {
                session.transport_path = None;
            }
            "org.bluez.Device1" => {
                // Unpaired/removed device.
                refresh_devices(ctx).await;
            }
            _ => {}
        }
    }
}

async fn handle_properties_changed(
    ctx: &Ctx,
    session: &mut Session,
    path: &str,
    iface: &str,
    changed: &HashMap<String, Value<'_>>,
) {
    match iface {
        "org.bluez.MediaPlayer1" => {
            if session.player_path.as_ref().map(|p| p.as_str()) != Some(path) {
                // A different (or first) player became chatty — adopt it.
                if let Ok(p) = ObjectPath::try_from(path.to_string()) {
                    session.player_path = Some(p.into());
                    adopt_device_of(ctx, session, path).await;
                }
            }
            let mut dirty = false;
            if let Some(track) = changed.get("Track").and_then(dict_from_value_ref) {
                apply_track_dict(session, &track);
                dirty = true;
            }
            if let Some(status) = changed.get("Status").and_then(str_from_value_ref) {
                session.status = parse_status(&status);
                dirty = true;
            }
            if let Some(pos) = changed.get("Position").and_then(u32_from_value_ref) {
                session.position_ms = Some(pos);
                dirty = true;
            }
            if let Some(port) = changed.get("ObexPort").and_then(u16_from_value_ref) {
                tracing::info!(port, "player supports AVRCP cover art");
                session.obex_port = Some(port);
                prime_art_session(ctx, session);
                dirty = true;
            }
            if dirty {
                publish_track(ctx, session).await;
            }
        }
        "org.bluez.MediaTransport1" => {
            if let Some(v) = changed.get("Volume").and_then(u16_from_value_ref) {
                apply_phone_volume(&ctx.app, v).await;
            }
        }
        "org.bluez.Device1" => {
            if let Some(connected) = changed.get("Connected").and_then(bool_from_value_ref) {
                if !connected && session.device_path.as_deref() == Some(path) {
                    tracing::info!(%path, "device disconnected");
                    clear_session(ctx, session).await;
                    session.device_path = None;
                    session.device_alias = None;
                }
            }
            // A pairing completed. Crucially, do NOT touch the adapter yet:
            // yanking Pairable/Discoverable at the Paired event interrupts
            // iOS's still-running post-pair profile discovery (the control
            // experiment with bt-agent — which never touches the adapter —
            // paired perfectly; we didn't). Trust the device (the explicit
            // pairing window is the consent), give the source time to bring
            // A2DP up on its own, dial it ourselves if it stays passive
            // (macOS), and only then close the pairing window.
            if changed.get("Paired").and_then(bool_from_value_ref) == Some(true) {
                tracing::info!(%path, "device paired; trusting");
                if let Ok(p) = ObjectPath::try_from(path.to_string()) {
                    let conn = ctx.conn.clone();
                    let app = ctx.app.clone();
                    let adapter = session.adapter_path.clone();
                    tokio::spawn(async move {
                        let result: anyhow::Result<()> = async {
                            let device = Device1Proxy::builder(&conn)
                                .path(p)?
                                .build()
                                .await?;
                            device.set_trusted(true).await?;
                            // Wait for the source's own A2DP setup.
                            let dev_path = device.inner().path().to_string();
                            let mut transport_up = false;
                            for _ in 0..16 {
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                if has_media_transport(&conn, &dev_path).await {
                                    transport_up = true;
                                    break;
                                }
                            }
                            if !transport_up {
                                // Passive source (macOS): dial it ourselves.
                                tracing::info!("no audio transport after pairing; connecting back");
                                if let Err(err) = device.disconnect().await {
                                    tracing::debug!(%err, "pre-connect disconnect (may not be up)");
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                device.connect().await?;
                            }
                            tracing::info!(transport_up, "post-pair settled; closing pairing window");
                            Ok(())
                        }
                        .await;
                        if let Err(err) = result {
                            tracing::warn!(%err, "post-pair trust/connect failed");
                        }
                        // Close the pairing window regardless of outcome.
                        if let Some(adapter) = adapter {
                            let close = async {
                                let a = Adapter1Proxy::builder(&conn)
                                    .path(adapter)?
                                    .build()
                                    .await?;
                                a.set_pairable(false).await?;
                                a.set_discoverable(false).await
                            }
                            .await;
                            if let Err(err) = close {
                                tracing::warn!(%err, "failed to close pairing window");
                            }
                        }
                        crate::bt_agent::set_pairing(&app, Pairing::default()).await;
                    });
                }
            }
            if ["Connected", "Paired", "Alias"]
                .iter()
                .any(|k| changed.contains_key(*k))
            {
                refresh_devices(ctx).await;
            }
        }
        _ => {}
    }
}

async fn handle_command(ctx: &Ctx, session: &Session, cmd: SourceCommand) -> anyhow::Result<()> {
    match cmd {
        SourceCommand::SetVolume(level) => {
            // System (PipeWire) volume...
            crate::audio::set_system_volume(level).await?;
            // ...and AVRCP absolute volume on the phone, like v1.
            if let Some(path) = &session.transport_path {
                let proxy = MediaTransport1Proxy::builder(&ctx.conn)
                    .path(path.clone())?
                    .build()
                    .await?;
                let _ = proxy.set_volume((level * AVRCP_MAX).round() as u16).await;
            }
            ctx.app.shared.write().await.volume = level;
            ctx.app.broadcast(ServerMessage::Volume { level });
        }
        transport => {
            let Some(path) = &session.player_path else {
                anyhow::bail!("no active player");
            };
            let player = MediaPlayer1Proxy::builder(&ctx.conn)
                .path(path.clone())?
                .build()
                .await?;
            match transport {
                SourceCommand::Play => player.play().await?,
                SourceCommand::Pause => player.pause().await?,
                SourceCommand::Next => player.next().await?,
                SourceCommand::Previous => player.previous().await?,
                SourceCommand::SetVolume(_) => unreachable!(),
            }
        }
    }
    Ok(())
}

/// Resolve and publish the device (phone) owning an object like
/// `/org/bluez/hci0/dev_XX_.../player0`.
/// Minimal source arbitration until the Phase-3 source manager lands:
/// iOS keeps mirroring its now-playing state over AVRCP while the audio
/// actually routes to AirPlay (or elsewhere), so while a non-Bluetooth
/// source holds the display, the phone's AVRCP chatter must not clobber
/// the track/source state. Session bookkeeping still runs; only the
/// shared-state writes are gated.
async fn bt_owns_display(app: &SharedApp) -> bool {
    matches!(
        app.shared.read().await.source.active,
        None | Some(SourceKind::Bluetooth)
    )
}

async fn adopt_device_of(ctx: &Ctx, session: &mut Session, child_path: &str) {
    let Some(device_path) = device_prefix(child_path) else {
        return;
    };
    let alias = match ObjectPath::try_from(device_path.clone()) {
        Ok(p) => match Device1Proxy::builder(&ctx.conn).path(p) {
            Ok(b) => match b.build().await {
                Ok(proxy) => proxy.alias().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        },
        Err(_) => None,
    };
    session.device_path = Some(device_path);
    session.device_alias = alias.clone();

    if !bt_owns_display(&ctx.app).await {
        return;
    }
    let source = SourceInfo {
        active: Some(SourceKind::Bluetooth),
        device_name: alias,
    };
    ctx.app.shared.write().await.source = source.clone();
    ctx.app.broadcast(ServerMessage::Source(source));
}

async fn handle_bt_command(
    ctx: &Ctx,
    session: &Session,
    decision: &crate::bt_agent::DecisionSlot,
    cmd: BtCommand,
) {
    let resolve = |confirmed: bool| {
        if let Some(tx) = decision.lock().unwrap().take() {
            let _ = tx.send(confirmed);
        }
    };
    match cmd {
        BtCommand::Pairing(PairingAction::Enable) => {
            // Entering pairing mode releases current connections: the user
            // is explicitly adding a new device, and the dongle struggles
            // to accept pairings while servicing an A2DP link anyway.
            disconnect_all(ctx, session).await;
            if set_discoverable(ctx, session, true).await {
                crate::bt_agent::set_pairing(
                    &ctx.app,
                    Pairing {
                        state: PairingState::Discoverable,
                        ..Pairing::default()
                    },
                )
                .await;
            }
        }
        BtCommand::Pairing(PairingAction::Cancel) => {
            resolve(false);
            set_discoverable(ctx, session, false).await;
            crate::bt_agent::set_pairing(&ctx.app, Pairing::default()).await;
        }
        BtCommand::Pairing(PairingAction::Confirm) => resolve(true),
        BtCommand::Pairing(PairingAction::Reject) => resolve(false),
        BtCommand::Device { address, action } => {
            device_action(ctx, session, &address, action).await;
        }
    }
}

/// Does a MediaTransport1 (active audio profile) exist under `dev_path`?
async fn has_media_transport(conn: &zbus::Connection, dev_path: &str) -> bool {
    let Ok(om) = ObjectManagerProxy::builder(conn)
        .destination("org.bluez")
        .and_then(|b| b.path("/"))
        .map(|b| b.build())
    else {
        return false;
    };
    let Ok(om) = om.await else { return false };
    let Ok(objects) = om.get_managed_objects().await else {
        return false;
    };
    objects.iter().any(|(path, ifaces)| {
        path.as_str().starts_with(dev_path)
            && ifaces
                .keys()
                .any(|i| i.as_str() == "org.bluez.MediaTransport1")
    })
}

/// Disconnect every connected device (entering pairing mode).
async fn disconnect_all(ctx: &Ctx, session: &Session) {
    let connected: Vec<String> = ctx
        .app
        .shared
        .read()
        .await
        .bt_devices
        .iter()
        .filter(|d| d.connected)
        .map(|d| d.address.clone())
        .collect();
    for address in connected {
        tracing::info!(%address, "disconnecting for pairing mode");
        device_action(ctx, session, &address, BtDeviceAction::Disconnect).await;
    }
}

/// Toggle adapter discoverability (+ pairability). Returns success.
async fn set_discoverable(ctx: &Ctx, session: &Session, on: bool) -> bool {
    let Some(path) = &session.adapter_path else {
        tracing::warn!("no BT adapter; cannot toggle discoverable");
        return false;
    };
    let result = async {
        let adapter = Adapter1Proxy::builder(&ctx.conn)
            .path(path.clone())?
            .build()
            .await?;
        adapter.set_pairable(on).await?;
        adapter.set_discoverable(on).await
    }
    .await;
    match result {
        Ok(()) => {
            tracing::info!(discoverable = on, "adapter discoverability changed");
            true
        }
        Err(err) => {
            tracing::warn!(%err, on, "failed to toggle discoverable");
            false
        }
    }
}

/// Connect/disconnect/unpair a known device by address.
async fn device_action(ctx: &Ctx, session: &Session, address: &str, action: BtDeviceAction) {
    if !address
        .bytes()
        .all(|b| b.is_ascii_hexdigit() || b == b':')
    {
        tracing::warn!(%address, "ignoring bt action for malformed address");
        return;
    }
    let Some(adapter) = &session.adapter_path else {
        tracing::warn!("no BT adapter; cannot run device action");
        return;
    };
    let dev_path = format!("{}/dev_{}", adapter.as_str(), address.replace(':', "_"));
    tracing::info!(%address, ?action, "bt device action");
    // Connect means "switch to this device": the dongle can't service two
    // A2DP links (a second connect flaps and drops itself), so release any
    // other connected device first.
    let others: Vec<String> = if action == BtDeviceAction::Connect {
        ctx.app
            .shared
            .read()
            .await
            .bt_devices
            .iter()
            .filter(|d| d.connected && d.address != address)
            .map(|d| format!("{}/dev_{}", adapter.as_str(), d.address.replace(':', "_")))
            .collect()
    } else {
        Vec::new()
    };
    let conn = ctx.conn.clone();
    let adapter = adapter.clone();
    // Connect can block for many seconds; never stall the event loop.
    tokio::spawn(async move {
        let result: anyhow::Result<()> = async {
            let path = ObjectPath::try_from(dev_path.clone())?;
            match action {
                BtDeviceAction::Connect => {
                    for other in &others {
                        tracing::info!(device = %other, "disconnecting for device switch");
                        let disconnect = async {
                            let path = ObjectPath::try_from(other.clone())?;
                            Device1Proxy::builder(&conn)
                                .path(path)?
                                .build()
                                .await?
                                .disconnect()
                                .await?;
                            anyhow::Ok(())
                        }
                        .await;
                        if let Err(err) = disconnect {
                            tracing::warn!(%err, "switch disconnect failed");
                        }
                    }
                    if !others.is_empty() {
                        // Let the radio settle before dialing the new link.
                        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    }
                    Device1Proxy::builder(&conn)
                        .path(path)?
                        .build()
                        .await?
                        .connect()
                        .await?
                }
                BtDeviceAction::Disconnect => {
                    Device1Proxy::builder(&conn)
                        .path(path)?
                        .build()
                        .await?
                        .disconnect()
                        .await?
                }
                BtDeviceAction::Remove => {
                    Adapter1Proxy::builder(&conn)
                        .path(adapter)?
                        .build()
                        .await?
                        .remove_device(&path)
                        .await?
                }
            }
            Ok(())
        }
        .await;
        if let Err(err) = result {
            tracing::warn!(%err, ?action, "bt device action failed");
        }
    });
}

/// Re-enumerate paired devices and broadcast when the list changed.
async fn refresh_devices(ctx: &Ctx) {
    let devices = match enumerate_devices(&ctx.conn).await {
        Ok(devices) => devices,
        Err(err) => {
            tracing::warn!(%err, "device enumeration failed");
            return;
        }
    };
    let mut s = ctx.app.shared.write().await;
    if s.bt_devices != devices {
        s.bt_devices = devices.clone();
        drop(s);
        ctx.app.broadcast(ServerMessage::BtDevices { devices });
    }
}

async fn enumerate_devices(conn: &zbus::Connection) -> anyhow::Result<Vec<BtDevice>> {
    let om = ObjectManagerProxy::builder(conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    let mut out = Vec::new();
    for (_path, interfaces) in om.get_managed_objects().await? {
        for (iface, props) in &interfaces {
            if iface.as_str() != "org.bluez.Device1" {
                continue;
            }
            if props.get("Paired").and_then(bool_from_value) != Some(true) {
                continue;
            }
            let Some(address) = props.get("Address").and_then(str_from_value) else {
                continue;
            };
            let name = props
                .get("Alias")
                .and_then(str_from_value)
                .unwrap_or_else(|| address.clone());
            let connected = props
                .get("Connected")
                .and_then(bool_from_value)
                .unwrap_or(false);
            out.push(BtDevice {
                address,
                name,
                connected,
            });
        }
    }
    // Connected first, then alphabetical.
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// Set the BlueZ controller alias — the advertised speaker name.
async fn apply_adapter_alias(ctx: &Ctx, session: &Session, name: &str) {
    let Some(path) = &session.adapter_path else {
        tracing::debug!("no BT adapter yet; alias not set");
        return;
    };
    let result = async {
        Adapter1Proxy::builder(&ctx.conn)
            .path(path.clone())?
            .build()
            .await?
            .set_alias(name)
            .await
    }
    .await;
    match result {
        Ok(()) => tracing::info!(%name, "BT adapter alias set"),
        Err(err) => tracing::warn!(%err, %name, "failed to set BT adapter alias"),
    }
}

async fn clear_session(ctx: &Ctx, session: &mut Session) {
    *session = Session {
        adapter_path: session.adapter_path.clone(),
        device_path: session.device_path.clone(),
        device_alias: session.device_alias.clone(),
        ..Session::default()
    };
    // Image handles are namespaced per device; drop stale mappings.
    ctx.resolved.lock().unwrap().clear();
    let mut s = ctx.app.shared.write().await;
    // Only clear the display if Bluetooth owns it — a BT disconnect must
    // not wipe another source's active session.
    if !matches!(s.source.active, None | Some(SourceKind::Bluetooth)) {
        return;
    }
    s.track = None;
    s.source = SourceInfo::default();
    drop(s);
    ctx.app
        .broadcast(ServerMessage::Source(SourceInfo::default()));
}

async fn publish_track(ctx: &Ctx, session: &mut Session) {
    if !bt_owns_display(&ctx.app).await {
        return;
    }
    maybe_request_art(ctx, session);
    let track = session.to_track(&ctx.resolved);
    ctx.app.shared.write().await.track = Some(track.clone());
    ctx.app.broadcast(ServerMessage::Track(track));
}

/// Kick off a cover-art fetch when the player advertises BIP support and
/// the current track has an unresolved image handle.
fn maybe_request_art(ctx: &Ctx, session: &mut Session) {
    let (Some(port), Some(handle)) = (session.obex_port, session.img_handle.clone()) else {
        return;
    };
    if session.art_requested_for.as_ref() == Some(&handle)
        || ctx.resolved.lock().unwrap().contains_key(&handle)
    {
        return;
    }
    let Some(address) = session.address() else {
        return;
    };
    session.art_requested_for = Some(handle.clone());
    let _ = ctx.art_tx.send(crate::artwork::ArtRequest {
        address,
        psm: port,
        handle: Some(handle),
    });
}

/// Eagerly establish the BIP session as soon as cover-art support is seen —
/// phones only include `ImgHandle` in metadata while the session is alive.
fn prime_art_session(ctx: &Ctx, session: &Session) {
    let (Some(port), Some(address)) = (session.obex_port, session.address()) else {
        return;
    };
    let _ = ctx.art_tx.send(crate::artwork::ArtRequest {
        address,
        psm: port,
        handle: None,
    });
}

/// Phone changed its volume (AVRCP absolute volume, 0–127): follow with the
/// system volume and notify clients.
async fn apply_phone_volume(app: &SharedApp, avrcp: u16) {
    let level = (avrcp as f32 / AVRCP_MAX).clamp(0.0, 1.0);
    tracing::debug!(avrcp, level, "phone volume changed");
    if let Err(err) = crate::audio::set_system_volume(level).await {
        tracing::warn!(%err, "failed to set system volume");
    }
    app.shared.write().await.volume = level;
    app.broadcast(ServerMessage::Volume { level });
}

fn apply_track_dict(session: &mut Session, track: &HashMap<String, OwnedValue>) {
    session.title = track
        .get("Title")
        .and_then(str_from_value)
        .filter(|s| !s.is_empty());
    session.artist = track
        .get("Artist")
        .and_then(str_from_value)
        .filter(|s| !s.is_empty());
    session.album = track
        .get("Album")
        .and_then(str_from_value)
        .filter(|s| !s.is_empty());
    session.duration_ms = track.get("Duration").and_then(u32_from_value);
    // Cover art handle: only present while a BIP session is up (or on
    // phones that always include it); absent means no art for this track.
    session.img_handle = track
        .get("ImgHandle")
        .and_then(str_from_value)
        .filter(|s| !s.is_empty());
    // New track starts at the beginning unless BlueZ tells us otherwise.
    session.position_ms = Some(0);
}

fn parse_status(status: &str) -> PlaybackStatus {
    match status {
        "playing" => PlaybackStatus::Playing,
        "paused" => PlaybackStatus::Paused,
        "stopped" => PlaybackStatus::Stopped,
        "forward-seek" => PlaybackStatus::ForwardSeek,
        "reverse-seek" => PlaybackStatus::ReverseSeek,
        _ => PlaybackStatus::Error,
    }
}

/// `/org/bluez/hci0/dev_AA_BB_.../player0` → `/org/bluez/hci0/dev_AA_BB_...`
fn device_prefix(path: &str) -> Option<String> {
    let dev_start = path.find("/dev_")?;
    let rest = &path[dev_start + 1..];
    let dev_end = rest
        .find('/')
        .map(|i| dev_start + 1 + i)
        .unwrap_or(path.len());
    Some(path[..dev_end].to_string())
}

// --- zvariant extraction helpers -------------------------------------------

fn str_from_value(v: &OwnedValue) -> Option<String> {
    String::try_from(v.clone()).ok()
}

fn u32_from_value(v: &OwnedValue) -> Option<u32> {
    u32::try_from(v.clone()).ok()
}

fn bool_from_value(v: &OwnedValue) -> Option<bool> {
    bool::try_from(v.clone()).ok()
}

fn u16_from_value(v: &OwnedValue) -> Option<u16> {
    u16::try_from(v.clone()).ok()
}

fn dict_from_value(v: &OwnedValue) -> Option<HashMap<String, OwnedValue>> {
    HashMap::<String, OwnedValue>::try_from(v.clone()).ok()
}

fn str_from_value_ref(v: &Value<'_>) -> Option<String> {
    v.try_to_owned().ok().and_then(|o| str_from_value(&o))
}

fn u32_from_value_ref(v: &Value<'_>) -> Option<u32> {
    v.try_to_owned().ok().and_then(|o| u32_from_value(&o))
}

fn u16_from_value_ref(v: &Value<'_>) -> Option<u16> {
    v.try_to_owned().ok().and_then(|o| u16_from_value(&o))
}

fn bool_from_value_ref(v: &Value<'_>) -> Option<bool> {
    v.try_to_owned().ok().and_then(|o| bool::try_from(o).ok())
}

fn dict_from_value_ref(v: &Value<'_>) -> Option<HashMap<String, OwnedValue>> {
    v.try_to_owned().ok().and_then(|o| dict_from_value(&o))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_prefix_extraction() {
        assert_eq!(
            device_prefix("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/player0").as_deref(),
            Some("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF")
        );
        assert_eq!(
            device_prefix("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF").as_deref(),
            Some("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF")
        );
        assert_eq!(device_prefix("/org/bluez/hci0"), None);
    }

    #[test]
    fn status_parsing() {
        assert_eq!(parse_status("playing"), PlaybackStatus::Playing);
        assert_eq!(parse_status("forward-seek"), PlaybackStatus::ForwardSeek);
        assert_eq!(parse_status("bogus"), PlaybackStatus::Error);
    }
}
