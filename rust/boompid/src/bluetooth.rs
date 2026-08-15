//! Bluetooth audio source: BlueZ over D-Bus (zbus).
//!
//! Mirrors the v1 Node.js integration, but handles devices appearing at
//! runtime (v1 TODO) by driving everything from `ObjectManager` +
//! `PropertiesChanged` on the `org.bluez` service:
//!
//! - `org.bluez.Device1` - connect/disconnect + device alias
//! - `org.bluez.MediaPlayer1` - track metadata, playback status, transport
//! - `org.bluez.MediaTransport1` - AVRCP absolute volume (0-127)
//!
//! Plus the pairing agent (`bt_agent`, NoInputNoOutput/JustWorks with an
//! explicit pairing window as the consent) and cover art (obexd/BIP, see
//! `artwork`).

#![cfg(target_os = "linux")]

use crate::state::{now_ms, BtCommand, SharedApp, SourceCommand};
use boompi_proto::{
    BtDevice, BtDeviceAction, Pairing, PairingAction, PairingState, PlaybackStatus,
    ServerMessage, SourceInfo, SourceKind, Track,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::fdo::ObjectManagerProxy;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, MessageStream};

#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
trait Adapter1 {
    /// Controller alias - the name phones see when pairing/connecting.
    #[zbus(property)]
    fn set_alias(&self, alias: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_powered(&self, powered: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_discoverable(&self, discoverable: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_pairable(&self, pairable: bool) -> zbus::Result<()>;
    fn remove_device(&self, device: &ObjectPath<'_>) -> zbus::Result<()>;
    /// Inquiry scan: gamepads in pairing mode only advertise - the
    /// speaker must find them and initiate (the opposite of phones,
    /// which initiate toward the discoverable speaker).
    fn start_discovery(&self) -> zbus::Result<()>;
    fn stop_discovery(&self) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn connect_profile(&self, uuid: &str) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;
    /// Device ID profile identity, e.g. "bluetooth:v004Cp720Dd0F20"
    /// (v = vendor; 0x004C = Apple). Absent on devices without DI.
    #[zbus(property)]
    fn modalias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_trusted(&self, trusted: bool) -> zbus::Result<()>;
    fn pair(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn paired(&self) -> zbus::Result<bool>;
    /// bluez's device classification ("input-gaming" for gamepads,
    /// derived from the BR/EDR Class of Device or the BLE Appearance).
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
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
    /// BIP OBEX PSM for AVRCP cover art. `[experimental]` - present only
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

/// AVRCP absolute volume is 0-127.
const AVRCP_MAX: f32 = 127.0;

/// Mutable view of the currently connected phone/player.
#[derive(Default)]
struct Session {
    /// Local controller (survives phone disconnects).
    adapter_path: Option<OwnedObjectPath>,
    device_path: Option<String>,
    device_alias: Option<String>,
    device_address: Option<String>,
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
    // session state (signals keep flowing - the well-known name just
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

    // Recovery heartbeat: the loop is otherwise purely event-driven,
    // and a controller that vanishes emits exactly one event - if the
    // recovery attempt it triggers fails, nothing would ever retry.
    let mut health_tick = tokio::time::interval(std::time::Duration::from_secs(30));
    health_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = health_tick.tick() => {
                ensure_powered(&ctx, &session).await;
            }
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
            // Late-enumerating or hot-plugged adapter: advertise the
            // configured speaker name right away (startup ordering means
            // the boot-time alias set can race the adapter's appearance),
            // and clear any `Unavailable` pairing state shown to the UIs.
            ensure_powered(ctx, session).await;
            apply_adapter_alias(ctx, session, &ctx.app.speaker_name().await).await;
            crate::bt_agent::set_pairing(&ctx.app, Pairing::default()).await;
        }
        "org.bluez.Device1" => {
            refresh_devices(ctx).await;
            maybe_autopair_gamepad(ctx, session, path.clone()).await;
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
                apply_phone_volume(ctx, session, v).await;
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
            "org.bluez.Adapter1"
                if session
                    .adapter_path
                    .as_ref()
                    .is_some_and(|p| p.as_str() == path) =>
            {
                // The controller died outright (bench: the 6.6.78 urb
                // wedge can remove the hci entirely, not just leave it
                // unpowered). Clear the stale path so the health tick's
                // ensure_powered takes the no-adapter recovery branch
                // instead of no-oping against a dead proxy forever.
                tracing::warn!(%path, "BT adapter removed; USB recovery will retry");
                session.adapter_path = None;
                crate::bt_agent::set_pairing(
                    &ctx.app,
                    Pairing {
                        state: PairingState::Unavailable,
                        ..Pairing::default()
                    },
                )
                .await;
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
                // A different (or first) player became chatty - adopt it.
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
                apply_phone_volume(ctx, session, v).await;
            }
        }
        "org.bluez.Device1" => {
            // Re-discovered cached devices surface as property churn
            // (RSSI), not InterfacesAdded: give pads the same chance.
            if changed.contains_key("RSSI") {
                if let Ok(p) = ObjectPath::try_from(path.to_string()) {
                    maybe_autopair_gamepad(ctx, session, p.into()).await;
                }
            }
            if let Some(connected) = changed.get("Connected").and_then(bool_from_value_ref) {
                if !connected && session.device_path.as_deref() == Some(path) {
                    tracing::info!(%path, "device disconnected");
                    clear_session(ctx, session).await;
                    session.device_path = None;
                    session.device_alias = None;
                    session.device_address = None;
                }
            }
            // A pairing completed. Crucially, do NOT touch the adapter yet:
            // yanking Pairable/Discoverable at the Paired event interrupts
            // iOS's still-running post-pair profile discovery (the control
            // experiment with bt-agent - which never touches the adapter -
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
                            let device = Device1Proxy::builder(&conn).path(p)?.build().await?;
                            device.set_trusted(true).await?;
                            // Gamepads never produce an audio transport: the
                            // A2DP dial-back below would wait 8 s, disconnect
                            // the pad (a DualSense powers itself off when the
                            // host drops the link), and reconnect into thin
                            // air. The autopair path owns gamepad connection.
                            if device.icon().await.as_deref() == Ok("input-gaming") {
                                tracing::info!("gamepad paired; skipping audio dial-back");
                                return Ok(());
                            }
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
                                if device.connected().await.unwrap_or(false) {
                                    // Still connected, just no A2DP yet: bring
                                    // the profile up IN PLACE. Yanking a live
                                    // link mid-setup is how iOS pops
                                    // 'Connection Unsuccessful' and stops
                                    // page-scanning (bench: congested radio,
                                    // gamepad + pairing + USB audio sharing
                                    // one dwc_otg bus stretched setup past
                                    // the old 8s disconnect). AudioSource
                                    // only - a full connect() also pokes HFP,
                                    // which a speaker cannot answer.
                                    tracing::info!(
                                        "no audio transport after pairing; connecting A2DP in place"
                                    );
                                    device
                                        .connect_profile("0000110a-0000-1000-8000-00805f9b34fb")
                                        .await?;
                                } else {
                                    // Paired then dropped the link entirely:
                                    // the passive-sender pattern (macOS pairs
                                    // and waits to be dialed).
                                    tracing::info!(
                                        "no audio transport after pairing; dialing back"
                                    );
                                    device.connect().await?;
                                }
                            }
                            tracing::info!(
                                transport_up,
                                "post-pair settled; closing pairing window"
                            );
                            Ok(())
                        }
                        .await;
                        if let Err(err) = result {
                            tracing::warn!(%err, "post-pair trust/connect failed");
                        }
                        // Close the pairing window regardless of outcome.
                        if let Some(adapter) = adapter {
                            let close = async {
                                let a =
                                    Adapter1Proxy::builder(&conn).path(adapter)?.build().await?;
                                let _ = a.stop_discovery().await;
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
            // The music track's volume: the mixer applies it to the
            // streams (sink stays at reference). Echo to the sender's
            // transport so the phone's slider follows - proven safe:
            // with the hw-volume handshake off, senders display the
            // value without rescaling their PCM.
            ctx.app.shared.write().await.volume = level;
            ctx.app.broadcast(ServerMessage::Volume { level });
            if let Some(path) = &session.transport_path {
                let proxy = MediaTransport1Proxy::builder(&ctx.conn)
                    .path(path.clone())?
                    .build()
                    .await?;
                if let Err(err) = proxy.set_volume((level * AVRCP_MAX).round() as u16).await {
                    tracing::debug!(%err, ?path, "AVRCP set_volume echo failed");
                }
            }
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
    let (alias, address, modalias) = match ObjectPath::try_from(device_path.clone()) {
        Ok(p) => match Device1Proxy::builder(&ctx.conn).path(p) {
            Ok(b) => match b.build().await {
                Ok(proxy) => (
                    proxy.alias().await.ok(),
                    proxy.address().await.ok(),
                    proxy.modalias().await.ok(),
                ),
                Err(_) => (None, None, None),
            },
            Err(_) => (None, None, None),
        },
        Err(_) => (None, None, None),
    };
    session.device_path = Some(device_path);
    session.device_alias = alias.clone();
    session.device_address = address;

    if !bt_owns_display(&ctx.app).await {
        return;
    }
    let source = SourceInfo {
        active: Some(SourceKind::Bluetooth),
        device_name: alias,
        controllable: true,
    };
    ctx.app.shared.write().await.source = source.clone();
    ctx.app.broadcast(ServerMessage::Source(source));

    sync_transport_volume(ctx, session).await;
}

/// Push the music track's volume to a freshly adopted sender so its
/// slider starts in sync and the loudness never jumps when switching
/// sources. (The senders display the value without rescaling their
/// PCM - the hw-volume handshake is off; see the wireplumber conf.)
async fn sync_transport_volume(ctx: &Ctx, session: &Session) {
    let Some(path) = &session.transport_path else {
        return;
    };
    let level = ctx.app.shared.read().await.volume;
    let sync = async {
        let proxy = MediaTransport1Proxy::builder(&ctx.conn)
            .path(path.clone())?
            .build()
            .await?;
        proxy.set_volume((level * AVRCP_MAX).round() as u16).await
    }
    .await;
    match sync {
        Ok(()) => tracing::info!(level, "sender volume synced to music track"),
        Err(err) => tracing::debug!(%err, "sender volume sync failed"),
    }
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
            // A wedged dongle presents as a present-but-unpowered
            // adapter; recover it before opening the pairing window.
            ensure_powered(ctx, session).await;
            // No adapter (dongle unplugged, bluetoothd down): say so
            // instead of silently doing nothing - a dead pairing button
            // is indistinguishable from a bug.
            if session.adapter_path.is_none() {
                tracing::warn!("pairing requested but no BT adapter present");
                crate::bt_agent::set_pairing(
                    &ctx.app,
                    Pairing {
                        state: PairingState::Unavailable,
                        ..Pairing::default()
                    },
                )
                .await;
                return;
            }
            // Entering pairing mode releases current connections: the user
            // is explicitly adding a new device, and the dongle struggles
            // to accept pairings while servicing an A2DP link anyway.
            // (Gamepads are spared - see disconnect_all.)
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
                // Phones initiate toward us; gamepads only advertise, so
                // finding a NEW pad needs an active inquiry scan. But
                // active inquiry competes with the inquiry/page scans
                // that let a phone find and connect to us - with an ACL
                // link also live (a connected gamepad), phone pairing
                // reliably failed on the bench until the pad was
                // disconnected. Only hunt for pads when nothing is
                // connected; pairing a second pad requires disconnecting
                // the first (rare, documented trade).
                let any_connected = ctx
                    .app
                    .shared
                    .read()
                    .await
                    .bt_devices
                    .iter()
                    .any(|d| d.connected);
                if any_connected {
                    tracing::info!(
                        "pairing window open; skipping gamepad discovery (device connected)"
                    );
                } else {
                    set_discovery(ctx, session, true).await;
                }
            }
        }
        BtCommand::Pairing(PairingAction::Cancel) => {
            resolve(false);
            set_discovery(ctx, session, false).await;
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

/// Disconnect connected AUDIO devices (entering pairing mode). The
/// dongle struggles to accept pairings while servicing an A2DP link,
/// but a HID link is featherweight - and disconnecting a gamepad
/// kills the player's inputs mid-game (pads do not re-initiate after
/// a host-side disconnect; the first bench attempt to pair a phone
/// during gameplay did exactly this).
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
        if let Some(adapter) = &session.adapter_path {
            let dev_path = format!("{}/dev_{}", adapter.as_str(), address.replace(':', "_"));
            let icon = async {
                let path = ObjectPath::try_from(dev_path).ok()?;
                let device = Device1Proxy::builder(&ctx.conn)
                    .path(path)
                    .ok()?
                    .build()
                    .await
                    .ok()?;
                device.icon().await.ok()
            }
            .await;
            if icon.as_deref() == Some("input-gaming") {
                tracing::info!(%address, "keeping gamepad connected through pairing mode");
                continue;
            }
        }
        tracing::info!(%address, "disconnecting for pairing mode");
        device_action(ctx, session, &address, BtDeviceAction::Disconnect).await;
    }
}

/// Toggle adapter discoverability (+ pairability). Returns success.
/// One auto-pair attempt at a time (discovery can surface a pad via
/// several property events in quick succession).
static AUTOPAIR_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A discovered device during an open pairing window: if it is a
/// gamepad, pair/trust/connect it - pads only advertise, they never
/// initiate toward us the way phones do.
async fn maybe_autopair_gamepad(ctx: &Ctx, session: &Session, path: OwnedObjectPath) {
    // Only while the user has the pairing window open.
    let window_open = matches!(
        ctx.app.shared.read().await.pairing.state,
        PairingState::Discoverable
    );
    if !window_open {
        return;
    }
    let Ok(builder) = Device1Proxy::builder(&ctx.conn).path(path.clone()) else {
        return;
    };
    let Ok(device) = builder.build().await else {
        return;
    };
    // bluez classifies gamepads as "input-gaming" from the BR/EDR
    // Class of Device or the BLE Appearance value.
    if device.icon().await.as_deref() != Ok("input-gaming") {
        return;
    }
    if AUTOPAIR_BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let alias = device.alias().await.unwrap_or_else(|_| "gamepad".into());
    // A pad we already know reappearing while the window is open is
    // not a pairing candidate - the user is adding something else.
    // Reconnect it quietly if needed and leave the window alone
    // (closing it here is how a powered-on pad used to slam the door
    // on the phone the user was actually trying to pair).
    if device.paired().await.unwrap_or(false) {
        if !device.connected().await.unwrap_or(false) {
            tracing::info!(%alias, "known gamepad seen during pairing window; reconnecting");
            if let Err(err) = device.connect().await {
                tracing::debug!(%err, %alias, "known gamepad reconnect failed");
            }
            refresh_devices(ctx).await;
        }
        AUTOPAIR_BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
        return;
    }
    tracing::info!(%alias, "gamepad discovered; pairing");
    // Informational only - autopair asks nobody anything. Broadcasting
    // Confirm here flashed a Pair/Reject dialog that auto-resolved
    // before a human could read it.
    crate::bt_agent::set_pairing(
        &ctx.app,
        Pairing {
            state: PairingState::Pairing,
            device_name: Some(alias.clone()),
            ..Pairing::default()
        },
    )
    .await;
    let result = async {
        device.pair().await?;
        device.set_trusted(true).await?;
        device.connect().await
    }
    .await;
    match result {
        Ok(()) => tracing::info!(%alias, "gamepad paired + connected"),
        Err(err) => tracing::warn!(%err, %alias, "gamepad pairing failed"),
    }
    // Close the window either way (mirrors the phone flow); leave it
    // to the user to reopen on failure.
    set_discovery(ctx, session, false).await;
    set_discoverable(ctx, session, false).await;
    crate::bt_agent::set_pairing(&ctx.app, Pairing::default()).await;
    refresh_devices(ctx).await;
    AUTOPAIR_BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Start/stop the inquiry scan that finds pairing-mode gamepads.
/// Failures are logged, not fatal: phone pairing works without it.
async fn set_discovery(ctx: &Ctx, session: &Session, on: bool) {
    let Some(path) = &session.adapter_path else {
        return;
    };
    let result = async {
        let a = Adapter1Proxy::builder(&ctx.conn)
            .path(path.clone())?
            .build()
            .await?;
        if on {
            a.start_discovery().await
        } else {
            a.stop_discovery().await
        }
    }
    .await;
    match result {
        Ok(()) => tracing::info!(scanning = on, "gamepad discovery toggled"),
        // "No discovery started"/"InProgress" are normal on repeats.
        Err(err) => tracing::debug!(%err, on, "discovery toggle"),
    }
}

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
    if !address.bytes().all(|b| b.is_ascii_hexdigit() || b == b':') {
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
    let app = ctx.app.clone();
    let forget_address = address.to_string();
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
                        .await?;
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

/// Make sure the controller is actually powered - AutoEnable does this
/// on healthy hardware, but the Pi 3 box's CSR8510 dongle (UB400)
/// sometimes wedges at the HCI transport (HCI_Reset times out, mgmt
/// Set Powered fails) and only a USB-level reset revives it. Detect
/// the powered-off + power-on-fails combination and perform the reset
/// (sysfs `authorized` toggle) that a human would otherwise do by
/// replugging the dongle.
async fn ensure_powered(ctx: &Ctx, session: &Session) {
    // Fast path: adapter present and healthy.
    if let Some(path) = &session.adapter_path {
        if let Ok(builder) = Adapter1Proxy::builder(&ctx.conn).path(path.clone()) {
            if let Ok(adapter) = builder.build().await {
                if adapter.powered().await.unwrap_or(false) {
                    RESET_FAILURES.store(0, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
                if adapter.set_powered(true).await.is_ok() {
                    tracing::info!("BT adapter powered on");
                    RESET_FAILURES.store(0, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
            }
        }
    }
    // Everything else - adapter refusing power, adapter object gone
    // (the 6.6.78 urb wedge can remove the hci outright), stale D-Bus
    // path - falls through to USB-level recovery.
    //
    // Exponential backoff between reset attempts: a dongle that is
    // wedged beyond software recovery (bench: dwc_otg-level FSM
    // timeouts only a power cycle clears) must not be USB-reset every
    // few seconds forever - 14 hours of 4-second resets thrashed the
    // shared USB bus for nothing.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now < NEXT_RESET_AT.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let failures = RESET_FAILURES.load(std::sync::atomic::Ordering::Relaxed);
    // Escalation: the `authorized` toggle clears transport-level
    // wedges, but the dwc_otg-level ones (urb resubmit failures at
    // boot on 6.6.78; bench 2-of-3 boots) survive it - only cutting
    // port power does the job (verified live: the hub's per-port
    // `disable` attribute revived a wedge the softer resets could
    // not). Port power on the Pi 3 hub is ganged, so sibling devices
    // (USB audio) re-enumerate too - acceptable while BT is dead.
    let escalate = failures >= 2;
    let acted = tokio::task::spawn_blocking(move || recover_bt_usb(escalate))
        .await
        .unwrap_or(0);
    if acted == 0 {
        // Nothing USB-shaped to recover: a box with no dongle (onboard
        // UART controller, or genuinely no Bluetooth). Not an error and
        // not worth burning backoff slots - stay quiet and cheap.
        tracing::debug!("no USB Bluetooth device found to recover");
        return;
    }
    let delay = 4u64.saturating_mul(1 << failures.min(8)).min(600);
    RESET_FAILURES.store(failures + 1, std::sync::atomic::Ordering::Relaxed);
    NEXT_RESET_AT.store(now + delay, std::sync::atomic::Ordering::Relaxed);
    tracing::warn!(
        attempt = failures + 1,
        next_retry_secs = delay,
        devices = acted,
        adapter_present = session.adapter_path.is_some(),
        "BT unhealthy; USB-level recovery attempted"
    );
    // Re-enumeration takes a few seconds; bluetoothd re-adds the
    // adapter (InterfacesAdded) and the handler / health tick brings
    // it up, resetting the counters via the fast path above.
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
}

/// Blocking USB-level Bluetooth recovery. Returns how many devices or
/// stuck ports were acted on (0 = nothing found - no USB BT hardware).
///
/// Candidates come from two directions, because the wedge decides what
/// survives: the /sys/class/bluetooth walk (hci still registered) and
/// a scan of USB devices advertising the Wireless/Bluetooth class
/// (hci gone but the device still enumerated - the state the ladder
/// previously could not see). Errors are tolerated per candidate: a
/// device vanishing mid-walk must not abort recovery of the others.
fn recover_bt_usb(escalate: bool) -> usize {
    let mut acted = 0usize;

    // First, undo any hub port left disabled by an interrupted
    // escalation: the device is invisible while the port is off, so
    // this must precede the device scans.
    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let iface = entry.path();
            if !iface
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(':'))
            {
                continue;
            }
            if let Ok(ports) = std::fs::read_dir(&iface) {
                for port in ports.flatten() {
                    let name_ok = port
                        .file_name()
                        .to_str()
                        .is_some_and(|n| n.contains("-port"));
                    if !name_ok {
                        continue;
                    }
                    let disable = port.path().join("disable");
                    if std::fs::read_to_string(&disable)
                        .map(|v| v.trim() == "1")
                        .unwrap_or(false)
                    {
                        tracing::warn!(path = %disable.display(), "re-enabling stuck-disabled USB port");
                        if std::fs::write(&disable, "0").is_ok() {
                            acted += 1;
                        }
                    }
                }
            }
        }
    }

    // Candidate USB devices.
    let mut devices: Vec<std::path::PathBuf> = Vec::new();
    // Via registered hcis: hciN/device resolves to the usb INTERFACE
    // (e.g. 1-1.2:1.0); its parent is the device (1-1.2).
    if let Ok(entries) = std::fs::read_dir("/sys/class/bluetooth") {
        for entry in entries.flatten() {
            if let Ok(iface) = std::fs::canonicalize(entry.path().join("device")) {
                if let Some(dev) = iface.parent() {
                    devices.push(dev.to_path_buf());
                }
            }
        }
    }
    // Via USB device class: bDeviceClass e0 = Wireless Controller
    // (the CSR dongle), or any interface with bInterfaceClass e0.
    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let is_class_e0 = |p: &std::path::Path, f: &str| {
                std::fs::read_to_string(p.join(f))
                    .map(|v| v.trim().eq_ignore_ascii_case("e0"))
                    .unwrap_or(false)
            };
            if name.contains(':') {
                if is_class_e0(&path, "bInterfaceClass") {
                    if let Ok(dev) = std::fs::canonicalize(path.join("..")) {
                        devices.push(dev);
                    }
                }
            } else if is_class_e0(&path, "bDeviceClass") {
                devices.push(path);
            }
        }
    }
    devices.sort();
    devices.dedup();

    for dev in devices {
        acted += 1;
        if escalate {
            if let Some(port_disable) = hub_port_disable_path(&dev) {
                tracing::warn!(path = %port_disable.display(), "escalating: USB port power cycle");
                let _ = std::fs::write(&port_disable, "1");
                std::thread::sleep(std::time::Duration::from_millis(2000));
                let _ = std::fs::write(&port_disable, "0");
                continue;
            }
        }
        let auth = dev.join("authorized");
        if auth.exists() {
            // If a previous cycle was interrupted (daemon restart
            // between the writes), the device sits de-authorized -
            // recover that first instead of toggling deeper down.
            if std::fs::read_to_string(&auth)
                .map(|v| v.trim() == "0")
                .unwrap_or(false)
            {
                let _ = std::fs::write(&auth, "1");
                continue;
            }
            // Always attempt the re-authorize even if the
            // de-authorize failed: never strand the device off.
            let _ = std::fs::write(&auth, "0");
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let _ = std::fs::write(&auth, "1");
        }
    }
    acted
}

/// The hub port `disable` attribute for a usb device: the device name
/// (e.g. 1-1.2) encodes hub (1-1) and port (2), and the hub's own
/// interface dir holds the per-port controls:
/// .../1-1:1.0/1-1-port2/disable.
fn hub_port_disable_path(dev: &std::path::Path) -> Option<std::path::PathBuf> {
    let devname = dev.file_name()?.to_str()?;
    let (hub, port) = devname.rsplit_once('.')?;
    let path = dev
        .parent()?
        .join(format!("{hub}:1.0"))
        .join(format!("{hub}-port{port}"))
        .join("disable");
    path.exists().then_some(path)
}

/// Consecutive failed dongle recoveries; drives the reset backoff.
static RESET_FAILURES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Unix seconds before which no further USB reset is attempted.
static NEXT_RESET_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Set the BlueZ controller alias - the advertised speaker name.
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
    // Only clear the display if Bluetooth owns it - a BT disconnect must
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

/// Eagerly establish the BIP session as soon as cover-art support is seen -
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

/// The sender changed its volume (AVRCP absolute volume, 0 to 127):
/// it becomes the music track's volume, same as a DACP or Spirc
/// command. Senders stream full-scale PCM (the hw-volume handshake is
/// off - bench-proven), so the mixer's per-stream volume is the one
/// true attenuator. Echoes of our own transport writes bounce back
/// here and are dropped by apply_external_volume's no-op guard.
async fn apply_phone_volume(ctx: &Ctx, _session: &Session, avrcp: u16) {
    let level = (avrcp as f32 / AVRCP_MAX).clamp(0.0, 1.0);
    tracing::debug!(avrcp, level, "sender volume -> music track");
    ctx.app.apply_external_volume(level).await;
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
