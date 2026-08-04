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

use crate::state::{now_ms, SharedApp, SourceCommand};
use boompi_proto::{PlaybackStatus, ServerMessage, SourceInfo, SourceKind, Track};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::fdo::ObjectManagerProxy;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{MatchRule, MessageStream};

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
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
    let resolved: crate::artwork::ResolvedArt = Default::default();
    let art_tx = crate::artwork::spawn(app.clone(), resolved.clone());
    tokio::spawn(async move {
        let mut rx = rx;
        loop {
            if let Err(err) = run(&app, &resolved, &art_tx, &mut rx).await {
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
) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    tracing::info!("connected to system D-Bus, watching org.bluez");
    let ctx = Ctx {
        app: app.clone(),
        conn: conn.clone(),
        resolved: resolved.clone(),
        art_tx: art_tx.clone(),
    };

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

    let mut session = Session::default();

    // Adopt whatever is already connected (e.g. phone paired+connected
    // before boompid started).
    for (path, interfaces) in om.get_managed_objects().await? {
        for (iface, props) in &interfaces {
            handle_interface_added(&ctx, &mut session, &path, iface.as_str(), props).await;
        }
    }

    loop {
        tokio::select! {
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

    let source = SourceInfo {
        active: Some(SourceKind::Bluetooth),
        device_name: alias,
    };
    ctx.app.shared.write().await.source = source.clone();
    ctx.app.broadcast(ServerMessage::Source(source));
}

async fn clear_session(ctx: &Ctx, session: &mut Session) {
    *session = Session {
        device_path: session.device_path.clone(),
        device_alias: session.device_alias.clone(),
        ..Session::default()
    };
    // Image handles are namespaced per device; drop stale mappings.
    ctx.resolved.lock().unwrap().clear();
    let mut s = ctx.app.shared.write().await;
    s.track = None;
    s.source = SourceInfo::default();
    drop(s);
    ctx.app
        .broadcast(ServerMessage::Source(SourceInfo::default()));
}

async fn publish_track(ctx: &Ctx, session: &mut Session) {
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
