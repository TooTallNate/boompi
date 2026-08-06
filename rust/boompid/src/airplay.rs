//! AirPlay source — shairport-sync managed as a child process.
//!
//! boompid owns the shairport-sync lifecycle (config generation, spawn,
//! restart) so the receiver name always matches the speaker name and there
//! is no separate service to keep in sync. Integration is three-legged:
//!
//! - **Audio**: shairport's `pipe` backend writes raw 44.1 kHz s16 stereo PCM
//!   into a FIFO; we bridge it into `pw-cat --playback --raw` (same approach
//!   as the librespot sink). Works identically on shairport 3.3.9 (Buildroot)
//!   and 4.x (dev Pi), needs no Pulse shim, and PipeWire mixing feeds the
//!   visualizer for free.
//! - **Metadata/state**: the native `org.gnome.ShairportSync` D-Bus interface
//!   (system bus). We code against the 3.3.9 property set; 4.x is a strict
//!   superset (adds `ClientName`, used opportunistically for the device name).
//! - **Transport**: `RemoteControl.Play/Pause/Next/Previous` — shairport
//!   relays these to the phone over DACP, which spares us an mDNS resolver
//!   and a DACP HTTP client.
//!
//! AirPlay 2 needs nqptp (not packaged in Buildroot 2025.02), so this is
//! classic AirPlay for now — see docs/PLAN.md "AirPlay 2 vs classic".

#![cfg(target_os = "linux")]

use crate::state::{now_ms, SharedApp, SourceCommand};
use boompi_proto::{PlaybackStatus, ServerMessage, SourceInfo, SourceKind, Track};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedValue;

const CONF_PATH: &str = "/tmp/boompi-shairport.conf";
const FIFO_PATH: &str = "/tmp/boompi-airplay.pcm";
/// AirPlay PCM timestamps run at the RTP frame rate.
const FRAME_RATE: u64 = 44_100;

#[zbus::proxy(
    interface = "org.gnome.ShairportSync",
    default_service = "org.gnome.ShairportSync",
    default_path = "/org/gnome/ShairportSync"
)]
trait ShairportSync {
    /// True while an AirPlay session is connected.
    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;
}

#[zbus::proxy(
    interface = "org.gnome.ShairportSync.RemoteControl",
    default_service = "org.gnome.ShairportSync",
    default_path = "/org/gnome/ShairportSync"
)]
trait RemoteControl {
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;

    /// Ask the *sender* to change its volume (DACP `dmcp.device-volume`);
    /// the phone's slider follows and it echoes back via `AirplayVolume`.
    fn set_airplay_volume(&self, volume: f64) -> zbus::Result<()>;

    /// Sender-side volume in dB attenuation: 0 (max) … -30 (min),
    /// -144 = mute. With `ignore_volume_control` set, shairport leaves the
    /// PCM alone and this is purely a control signal (parity with AVRCP
    /// absolute volume on the Bluetooth path).
    #[zbus(property)]
    fn airplay_volume(&self) -> zbus::Result<f64>;
    /// "Playing" / "Paused" / "Stopped" / "Not Available".
    #[zbus(property)]
    fn player_state(&self) -> zbus::Result<String>;
    /// "start/current/end" RTP frame timestamps (44.1 kHz).
    #[zbus(property)]
    fn progress_string(&self) -> zbus::Result<String>;
    /// MPRIS-style dict: xesam:title/artist/album, mpris:length/artUrl.
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

pub fn spawn(app: SharedApp) {
    if !app.cfg.airplay.enabled {
        tracing::info!("AirPlay disabled by config");
        return;
    }
    // Probe for the binary up front: an appliance image always ships it, and
    // on a dev box a missing install shouldn't spam the restart loop.
    match std::process::Command::new("shairport-sync").arg("-V").output() {
        Ok(out) => {
            let version = String::from_utf8_lossy(&out.stdout);
            tracing::info!(version = %version.trim(), "shairport-sync found");
        }
        Err(err) => {
            tracing::error!(%err, "AirPlay unavailable: shairport-sync not found in PATH");
            return;
        }
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    app.register_source(SourceKind::Airplay, tx);
    tokio::spawn(async move {
        loop {
            let result = run_once(&app, &mut rx).await;
            clear_if_active(&app).await;
            match result {
                // Clean exits (speaker rename) restart almost immediately.
                Ok(()) => tokio::time::sleep(Duration::from_secs(1)).await,
                Err(err) => {
                    tracing::warn!(%err, "airplay source failed; restarting in 10s");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }
    });
}

async fn run_once(
    app: &SharedApp,
    cmds: &mut mpsc::UnboundedReceiver<SourceCommand>,
) -> anyhow::Result<()> {
    let name = app.speaker_name().await;
    let airplay_model = app.shared.read().await.settings.airplay_model.clone();
    let mut cfg_watch = app.subscribe_cfg();
    cfg_watch.mark_unchanged();
    write_config(&name, &airplay_model)?;
    make_fifo(Path::new(FIFO_PATH))?;

    let mut child = tokio::process::Command::new("shairport-sync")
        .args(["-c", CONF_PATH, "-u"]) // -u: log to stderr
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(log_stderr(stderr));
    }

    // PCM bridge runs for the lifetime of this session; aborted on return.
    let mut bridge = AbortOnDrop(tokio::spawn(audio_bridge(PathBuf::from(FIFO_PATH))));

    let conn = zbus::Connection::system().await?;
    wait_for_bus_name(&conn).await?;
    let sps = ShairportSyncProxy::new(&conn).await?;
    let rc = RemoteControlProxy::new(&conn).await?;
    tracing::info!(%name, "AirPlay receiver active (shairport-sync child)");

    let mut active_stream = sps.receive_active_changed().await;
    let mut state_stream = rc.receive_player_state_changed().await;
    let mut meta_stream = rc.receive_metadata_changed().await;
    let mut progress_stream = rc.receive_progress_string_changed().await;
    let mut volume_stream = rc.receive_airplay_volume_changed().await;

    let mut meta = MetaState::default();

    // Adopt an already-running session (e.g. boompid restarted mid-stream).
    if sps.active().await.unwrap_or(false) {
        claim_source(app, &rc).await;
        if let Ok(md) = rc.metadata().await {
            apply_metadata(app, &md, &mut meta).await;
        }
        if let Ok(state) = rc.player_state().await {
            apply_player_state(app, &rc, &state).await;
        }
        if let Ok(db) = rc.airplay_volume().await {
            apply_airplay_volume(app, db).await;
        }
    }
    loop {
        tokio::select! {
            status = child.wait() => {
                anyhow::bail!("shairport-sync exited: {:?}", status?);
            }
            res = &mut bridge.0 => {
                anyhow::bail!("airplay audio bridge ended: {res:?}");
            }
            Some(active) = active_stream.next() => {
                match active.get().await {
                    Ok(true) => {
                        claim_source(app, &rc).await;
                        // Snap the speaker to the sender's slider position.
                        if let Ok(db) = rc.airplay_volume().await {
                            apply_airplay_volume(app, db).await;
                        }
                    }
                    Ok(false) => {
                        // NB: also fires once at startup when the property
                        // cache primes with the initial `false`.
                        let was_active = app.shared.read().await.source.active
                            == Some(SourceKind::Airplay);
                        if was_active {
                            tracing::info!("AirPlay client disconnected");
                            clear_if_active(app).await;
                        }
                        meta = MetaState::default();
                    }
                    Err(_) => {}
                }
            }
            Some(state) = state_stream.next() => {
                if let Ok(state) = state.get().await {
                    apply_player_state(app, &rc, &state).await;
                }
            }
            Some(md) = meta_stream.next() => {
                if let Ok(md) = md.get().await {
                    apply_metadata(app, &md, &mut meta).await;
                }
            }
            Some(progress) = progress_stream.next() => {
                if let Ok(progress) = progress.get().await {
                    apply_progress(app, &progress).await;
                }
            }
            Some(v) = volume_stream.next() => {
                if let Ok(db) = v.get().await {
                    // Guard on active source: the property can twitch
                    // during session setup/teardown.
                    if app.shared.read().await.source.active == Some(SourceKind::Airplay) {
                        apply_airplay_volume(app, db).await;
                    }
                }
            }
            _ = cfg_watch.changed() => {
                tracing::info!("speaker renamed or AirPlay model changed; restarting receiver");
                // kill_on_drop tears the shairport child down with us.
                return Ok(());
            }
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { anyhow::bail!("command channel closed") };
                let result = match cmd {
                    SourceCommand::Play => rc.play().await,
                    SourceCommand::Pause => rc.pause().await,
                    SourceCommand::Next => rc.next().await,
                    SourceCommand::Previous => rc.previous().await,
                    // The bluetooth/audio path already set the system
                    // volume; here we just make the sender's slider follow.
                    SourceCommand::SetVolume(level) => {
                        rc.set_airplay_volume(level_to_airplay_db(level)).await
                    }
                };
                if let Err(err) = result {
                    tracing::warn!(%err, ?cmd, "airplay remote command failed");
                }
            }
        }
    }
}

/// Sender volume (dB attenuation, 0 max … -30 min, -144 mute) → 0..1.
fn airplay_db_to_level(db: f64) -> f32 {
    if db <= -140.0 {
        return 0.0; // mute sentinel
    }
    (((db + 30.0) / 30.0) as f32).clamp(0.0, 1.0)
}

/// 0..1 → sender volume in dB (never the -144 mute sentinel; 0.0 maps to
/// the -30 dB floor so unmuting from the phone still works).
fn level_to_airplay_db(level: f32) -> f64 {
    (f64::from(level.clamp(0.0, 1.0)) * 30.0) - 30.0
}

/// The AirPlay sender moved its volume: follow with the system volume.
async fn apply_airplay_volume(app: &SharedApp, db: f64) {
    let level = airplay_db_to_level(db);
    tracing::debug!(db, level, "airplay sender volume changed");
    app.apply_external_volume(level).await;
}

/// Aborts the wrapped task when the session scope unwinds.
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Generated shairport-sync config (libconfig format).
fn write_config(name: &str, airplay_model: &str) -> anyhow::Result<()> {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    // Advertised model → the sender's AirPlay-picker icon (patched-in
    // shairport option; see buildroot/patches/shairport-sync). Empty =
    // shairport's default (generic speaker icon).
    let model_line = if airplay_model.is_empty() {
        String::new()
    } else {
        format!(
            "  airplay_device_model = \"{}\";\n",
            airplay_model.replace('\\', "\\\\").replace('"', "\\\"")
        )
    };
    let conf = format!(
        r#"// Generated by boompid — do not edit.
general = {{
  name = "{escaped}";
{model_line}  output_backend = "pipe";
  // Don't software-attenuate the PCM: the sender's volume drives the
  // system volume instead (AirplayVolume watcher), matching how AVRCP
  // absolute volume works on the Bluetooth path. Without this the
  // speaker has two volumes in series and the panel slider never moves.
  ignore_volume_control = "yes";
}};
pipe = {{
  name = "{FIFO_PATH}";
}};
metadata = {{
  enabled = "yes";
  include_cover_art = "yes";
}};
"#
    );
    std::fs::write(CONF_PATH, conf)?;
    Ok(())
}

fn make_fifo(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::FileTypeExt;
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.file_type().is_fifo() {
            return Ok(());
        }
        std::fs::remove_file(path)?;
    }
    let cpath = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    // SAFETY: cpath is a valid NUL-terminated path.
    if unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) } != 0 {
        anyhow::bail!("mkfifo {}: {}", path.display(), std::io::Error::last_os_error());
    }
    Ok(())
}

/// Wait for shairport-sync to claim its D-Bus name after spawning.
async fn wait_for_bus_name(conn: &zbus::Connection) -> anyhow::Result<()> {
    let dbus = zbus::fdo::DBusProxy::new(conn).await?;
    for _ in 0..30 {
        if dbus
            .name_has_owner("org.gnome.ShairportSync".try_into()?)
            .await
            .unwrap_or(false)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::bail!("org.gnome.ShairportSync never appeared on the system bus (dbus policy?)")
}

/// FIFO → `pw-cat --playback` (raw PCM pipe). One pw-cat per AirPlay
/// session: the read side blocks until shairport opens the pipe (session
/// start) and sees EOF when it closes it (session end).
///
/// NB: no `--raw` flag — it doesn't exist before PipeWire 1.4 and makes
/// 1.2.x print usage and exit. Stdin is always treated as a raw pipe
/// whose parameters come from the CLI args, and the rate REALLY matters:
/// pw-cat defaults to 48000, which plays 44100 content 8.8% fast (+1.5
/// semitones — the first bench test of this path shipped that).
async fn audio_bridge(fifo: PathBuf) -> anyhow::Result<()> {
    loop {
        // Blocks (on the blocking pool) until a writer appears.
        let mut pipe = tokio::fs::File::open(&fifo).await?;
        let mut pwcat = tokio::process::Command::new("pw-cat")
            .args([
                "--playback", "--rate", "44100", "--channels", "2", "--format", "s16", "-",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = pwcat
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("pw-cat stdin missing"))?;
        tracing::debug!("airplay audio session started");
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let n = pipe.read(&mut buf).await?;
            if n == 0 {
                break; // writer closed → session over
            }
            if stdin.write_all(&buf[..n]).await.is_err() {
                tracing::warn!("pw-cat died mid-session; will respawn");
                break;
            }
        }
        drop(stdin); // EOF → pw-cat drains and exits
        let _ = pwcat.wait().await;
        tracing::debug!("airplay audio session ended");
    }
}

async fn log_stderr(stderr: tokio::process::ChildStderr) {
    use tokio::io::AsyncBufReadExt;
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        tracing::debug!(target: "shairport", "{line}");
    }
}

async fn claim_source(app: &SharedApp, rc: &RemoteControlProxy<'_>) {
    // 4.x exposes the client's friendly name; 3.3.9 only has the IP, which
    // isn't worth showing.
    let device_name = rc
        .inner()
        .get_property::<String>("ClientName")
        .await
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "AirPlay".into());
    let source = SourceInfo {
        active: Some(SourceKind::Airplay),
        device_name: Some(device_name),
    };
    let mut s = app.shared.write().await;
    if s.source != source {
        tracing::info!(device = ?source.device_name, "AirPlay session active");
        s.source = source.clone();
        drop(s);
        app.broadcast(ServerMessage::Source(source));
    }
}

async fn clear_if_active(app: &SharedApp) {
    let mut s = app.shared.write().await;
    if s.source.active == Some(SourceKind::Airplay) {
        s.source = SourceInfo::default();
        s.track = None;
        drop(s);
        app.broadcast(ServerMessage::Source(SourceInfo::default()));
    }
}

async fn apply_player_state(app: &SharedApp, rc: &RemoteControlProxy<'_>, state: &str) {
    let status = match state {
        "Playing" => PlaybackStatus::Playing,
        "Paused" => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    };
    if status == PlaybackStatus::Playing {
        claim_source(app, rc).await;
    }
    let track = {
        let mut s = app.shared.write().await;
        if s.source.active != Some(SourceKind::Airplay) {
            return;
        }
        match s.track.as_mut() {
            Some(track) => {
                track.status = status;
                track.updated_at = now_ms();
                Some(track.clone())
            }
            None => None,
        }
    };
    if let Some(track) = track {
        app.broadcast(ServerMessage::Track(track));
    }
}

/// Per-session metadata bookkeeping for burst handling.
#[derive(Default)]
struct MetaState {
    /// `mpris:trackid` of the current track (stable per track).
    track_id: Option<String>,
    /// Cover-art cache path most recently scheduled for publishing.
    last_art_path: Option<String>,
}

/// Apply a `Metadata` property update.
///
/// shairport re-emits the dict several times per track and fills it in
/// incrementally (title first, artist/length/artUrl trickle in), so track
/// identity comes from `mpris:trackid` and absent fields merge with the
/// previous state — naively rebuilding the track on each burst reset the
/// position and wiped the artwork a few hundred ms after it was published.
async fn apply_metadata(app: &SharedApp, md: &HashMap<String, OwnedValue>, ms: &mut MetaState) {
    let title = md_str(md, "xesam:title");
    let track_id = md_track_id(md);
    if title.is_none() && track_id.is_none() {
        // shairport clears metadata between tracks; ignore empty dicts.
        return;
    }
    let new_track = match (&track_id, &ms.track_id) {
        (Some(new), Some(old)) => new != old,
        (Some(_), None) => true,
        // No trackid in this burst: assume same track unless the title says
        // otherwise below.
        (None, _) => false,
    };
    if track_id.is_some() {
        ms.track_id = track_id;
    }

    let track = {
        let mut s = app.shared.write().await;
        // Symmetric arbitration guard: only write the display while the
        // AirPlay session owns it (claimed on Active=true / Playing).
        if s.source.active != Some(SourceKind::Airplay) {
            return;
        }
        let prev = if new_track { None } else { s.track.take() };
        // Title change without a trackid is still a track change.
        let prev = match (&title, &prev) {
            (Some(new), Some(t)) if t.title.as_deref() != Some(new.as_str()) => None,
            _ => prev,
        };
        let fresh = prev.is_none();
        let base = prev.unwrap_or(Track {
            title: None,
            artist: None,
            album: None,
            duration_ms: None,
            position_ms: Some(0),
            status: s
                .track
                .as_ref()
                .map(|t| t.status)
                .unwrap_or(PlaybackStatus::Playing),
            artwork_id: None,
            updated_at: now_ms(),
        });
        let track = Track {
            title: title.or(base.title),
            artist: md_artist(md).or(base.artist),
            album: md_str(md, "xesam:album").or(base.album),
            duration_ms: md_length_ms(md).or(base.duration_ms),
            ..base
        };
        if fresh {
            // New track: the previous cover no longer applies.
            ms.last_art_path = None;
        }
        s.track = Some(track.clone());
        track
    };
    app.broadcast(ServerMessage::Track(track));

    if let Some(art) = md_str(md, "mpris:artUrl") {
        let path = art.strip_prefix("file://").unwrap_or(&art).to_string();
        if ms.last_art_path.as_deref() != Some(&path) {
            ms.last_art_path = Some(path.clone());
            publish_art_file(app.clone(), path);
        }
    }
}

async fn apply_progress(app: &SharedApp, progress: &str) {
    let Some((position_ms, duration_ms)) = parse_progress(progress) else {
        return;
    };
    let track = {
        let mut s = app.shared.write().await;
        if s.source.active != Some(SourceKind::Airplay) {
            return;
        }
        match s.track.as_mut() {
            Some(track) => {
                track.position_ms = Some(position_ms);
                if duration_ms > 0 {
                    track.duration_ms = Some(duration_ms);
                }
                track.updated_at = now_ms();
                Some(track.clone())
            }
            None => None,
        }
    };
    if let Some(track) = track {
        app.broadcast(ServerMessage::Track(track));
    }
}

/// Read shairport's cover-art cache file and publish it once it's whole.
///
/// The cache file is a raw buffer dump: it can be caught mid-write (the UI
/// was decoding truncated JPEGs) and even the finished file carries a
/// garbage tail after the image data (which breaks content-addressed
/// dedup). Poll until the bytes trim to a decodable image, then publish
/// only the image itself.
fn publish_art_file(app: SharedApp, path: String) {
    tokio::spawn(async move {
        for attempt in 0..12 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            let Ok(bytes) = tokio::fs::read(&path).await else {
                continue; // not written yet
            };
            let trimmed = crate::artwork::trim_image(&bytes);
            if trimmed.is_empty() {
                continue;
            }
            // Decode (cheap at cover sizes) to reject partial writes; the
            // trim alone can't tell a complete scan from a truncated one.
            if image::load_from_memory(trimmed).is_err() {
                continue;
            }
            tracing::info!(
                size = trimmed.len(),
                file = bytes.len(),
                %path,
                "airplay cover art"
            );
            crate::artwork::publish_current_art(
                &app,
                Bytes::copy_from_slice(trimmed),
                SourceKind::Airplay,
            )
            .await;
            return;
        }
        tracing::warn!(%path, "airplay cover art never became decodable; skipping");
    });
}

// ---------------------------------------------------------------------------
// Metadata parsing helpers
// ---------------------------------------------------------------------------

fn md_str(md: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    md.get(key)
        .and_then(|v| String::try_from(v.clone()).ok())
        .filter(|s| !s.is_empty())
}

/// `xesam:artist` is an array of strings per MPRIS; accept a bare string too.
fn md_artist(md: &HashMap<String, OwnedValue>) -> Option<String> {
    let v = md.get("xesam:artist")?;
    if let Ok(list) = <Vec<String>>::try_from(v.clone()) {
        let joined = list.join(", ");
        return (!joined.is_empty()).then_some(joined);
    }
    String::try_from(v.clone()).ok().filter(|s| !s.is_empty())
}

/// `mpris:trackid` is a D-Bus object path unique per track.
fn md_track_id(md: &HashMap<String, OwnedValue>) -> Option<String> {
    let v = md.get("mpris:trackid")?;
    if let Ok(p) = zbus::zvariant::OwnedObjectPath::try_from(v.clone()) {
        return Some(p.to_string());
    }
    String::try_from(v.clone()).ok()
}

/// `mpris:length` is int64 microseconds.
fn md_length_ms(md: &HashMap<String, OwnedValue>) -> Option<u32> {
    let v = md.get("mpris:length")?;
    let us = i64::try_from(v.clone())
        .ok()
        .or_else(|| u64::try_from(v.clone()).ok().and_then(|u| i64::try_from(u).ok()))?;
    if us <= 0 {
        return None;
    }
    u32::try_from(us / 1000).ok()
}

/// Parse `ProgressString` ("start/current/end" RTP frames @44.1 kHz) into
/// (position_ms, duration_ms).
fn parse_progress(s: &str) -> Option<(u32, u32)> {
    let mut parts = s.split('/');
    let start: u64 = parts.next()?.trim().parse().ok()?;
    let current: u64 = parts.next()?.trim().parse().ok()?;
    let end: u64 = parts.next()?.trim().parse().ok()?;
    let position_ms = current.saturating_sub(start) * 1000 / FRAME_RATE;
    let duration_ms = end.saturating_sub(start) * 1000 / FRAME_RATE;
    Some((
        u32::try_from(position_ms).ok()?,
        u32::try_from(duration_ms).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_string() {
        // 60 s track, 15 s in.
        let start = 1_000_000u64;
        let s = format!(
            "{start}/{}/{}",
            start + 15 * FRAME_RATE,
            start + 60 * FRAME_RATE
        );
        assert_eq!(parse_progress(&s), Some((15_000, 60_000)));
    }

    #[test]
    fn trims_jpeg_garbage_tail() {
        let mut buf = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9];
        buf.extend_from_slice(&[0xAA; 64]); // garbage tail
        assert_eq!(crate::artwork::trim_image(&buf), &buf[..9]);
        // Truncated JPEG (no EOI) is rejected outright.
        assert!(crate::artwork::trim_image(&[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]).is_empty());
    }

    #[test]
    fn trims_png_garbage_tail() {
        let mut buf = b"\x89PNG\r\n\x1a\n....chunks....IEND\xaeB`\x82".to_vec();
        let clean_len = buf.len();
        buf.extend_from_slice(&[0x55; 32]);
        assert_eq!(crate::artwork::trim_image(&buf).len(), clean_len);
        assert!(crate::artwork::trim_image(b"\x89PNG\r\n\x1a\nno-end-chunk").is_empty());
    }

    #[test]
    fn progress_string_garbage() {
        assert_eq!(parse_progress(""), None);
        assert_eq!(parse_progress("1/2"), None);
        assert_eq!(parse_progress("a/b/c"), None);
        // Out-of-order values must not panic.
        assert_eq!(parse_progress("100/50/20"), Some((0, 0)));
    }
}
