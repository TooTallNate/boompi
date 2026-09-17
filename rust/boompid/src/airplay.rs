//! AirPlay source - shairport-sync managed as a child process.
//!
//! boompid owns the shairport-sync lifecycle (config generation, spawn,
//! restart) so the receiver name always matches the speaker name and there
//! is no separate service to keep in sync. Integration is three-legged:
//!
//! - **Audio**: explicitly fixed 44.1 kHz S16_LE stereo PCM through a FIFO
//!   into `pw-cat --playback`. Shairport 5.x otherwise defaults to 48 kHz
//!   S32_LE in AP2 builds, which is incompatible with this raw bridge.
//! - **Metadata/state**: native `org.gnome.ShairportSync` on the system bus,
//!   plus live MPRIS Position for the 5.6-dev plist metadata path. Native
//!   progress still supplies classic duration and fallback position; existing
//!   4.x dev installs retain DACP availability and their classic-mode option.
//! - **Transport**: `RemoteControl.Play/Pause/Next/Previous` - shairport
//!   relays these over DACP or the AP2 event channel in the pinned development
//!   build (94070a1d). AP2 capabilities come from Client.CommandInformation,
//!   not RemoteControl.Available, which still only describes DACP.

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

// Same /run-not-/tmp reasoning as the FIFO below: tmpfiles-clean
// reaped the /tmp config after NTP jumped the clock forward. Benign
// (shairport reads it once per spawn, and every spawn rewrites it)
// but a debugging trap: a running `shairport-sync -c <path>` whose
// config "does not exist" looks much more broken than it is.
const CONF_PATH: &str = "/run/boompi/shairport.conf";
// In /run, NOT /tmp: the boxes boot with a wrong clock (no RTC), so
// boot-created files carry months-old timestamps until NTP lands, and
// systemd-tmpfiles' daily clean reaps anything in /tmp "older" than
// ten days - it deleted this FIFO mid-flight and AirPlay audio
// streamed into a dead inode (bench, Aug 2026). /run/boompi is a
// RuntimeDirectory: always present, never age-cleaned.
const FIFO_PATH: &str = "/run/boompi/airplay.pcm";
/// Legacy senders without SourceFormat metadata use 44.1 kHz RTP timestamps.
const FRAME_RATE: u32 = 44_100;

#[zbus::proxy(
    interface = "org.gnome.ShairportSync",
    default_service = "org.gnome.ShairportSync",
    default_path = "/org/gnome/ShairportSync"
)]
trait ShairportSync {
    /// True while an AirPlay session is connected.
    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn source_format(&self) -> zbus::Result<String>;
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

    /// Synchronize the sender's slider over DACP or the AP2 event channel.
    fn set_airplay_volume(&self, volume: f64) -> zbus::Result<()>;

    /// Sender-side volume in dB attenuation: 0 (max) … -30 (min),
    /// -144 = mute. With `ignore_volume_control` set, shairport leaves the
    /// PCM alone and this is purely a control signal (parity with AVRCP
    /// absolute volume on the Bluetooth path).
    #[zbus(property)]
    fn airplay_volume(&self) -> zbus::Result<f64>;
    /// DACP availability only, even in AP2-capable development builds.
    #[zbus(property)]
    fn available(&self) -> zbus::Result<bool>;
    /// "Buffered" / "Realtime" for AP2, "Classic" / "AirPlay" for classic.
    #[zbus(property)]
    fn stream_type(&self) -> zbus::Result<String>;
    /// "Playing" / "Paused" / "Stopped" / "Not Available".
    #[zbus(property)]
    fn player_state(&self) -> zbus::Result<String>;
    /// Legacy "start/current/end" RTP timestamps at the source sample rate.
    #[zbus(property)]
    fn progress_string(&self) -> zbus::Result<String>;
    /// MPRIS-style dict: xesam:title/artist/album, mpris:length/artUrl.
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[zbus::proxy(
    interface = "org.gnome.ShairportSync.Client",
    default_service = "org.gnome.ShairportSync",
    default_path = "/org/gnome/ShairportSync"
)]
trait AirplayClient {
    #[zbus(property(emits_changed_signal = "invalidates"))]
    fn command_information(&self) -> zbus::Result<Vec<OwnedValue>>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_service = "org.mpris.MediaPlayer2.ShairportSync",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MprisPlayer {
    /// Computed on Get, in microseconds; upstream never pushes changes.
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> zbus::Result<i64>;
}

pub fn spawn(app: SharedApp) {
    if !app.cfg.airplay.enabled {
        tracing::info!("AirPlay disabled by config");
        return;
    }
    // Probe for the binary up front: an appliance image always ships it, and
    // on a dev box a missing install shouldn't spam the restart loop.
    match std::process::Command::new("shairport-sync")
        .arg("-V")
        .output()
    {
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
    let (airplay_model, airplay_classic) = {
        let s = app.shared.read().await;
        (s.settings.airplay_model.clone(), s.settings.airplay_classic)
    };
    let mut cfg_watch = app.subscribe_cfg();
    cfg_watch.mark_unchanged();
    write_config(&name, &airplay_model, airplay_classic)?;
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
    let client = AirplayClientProxy::new(&conn).await?;
    // Older 4.x builds have neither Client nor a working live MPRIS Position.
    let plist_metadata = client.command_information().await.is_ok();
    let mpris = MprisPlayerProxy::builder(&conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let mut position_tick = tokio::time::interval(Duration::from_secs(1));
    position_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut position_warned = false;
    let mut live_position = false;
    tracing::info!(%name, "AirPlay receiver active (shairport-sync child)");

    let mut active_stream = sps.receive_active_changed().await;
    let mut state_stream = rc.receive_player_state_changed().await;
    let mut meta_stream = rc.receive_metadata_changed().await;
    let mut progress_stream = rc.receive_progress_string_changed().await;
    let mut volume_stream = rc.receive_airplay_volume_changed().await;
    let mut available_stream = rc.receive_available_changed().await;
    let mut commands_stream = client.receive_command_information_changed().await;
    let mut stream_type_stream = rc.receive_stream_type_changed().await;

    let mut meta = MetaState::default();

    // Adopt an already-running session (e.g. boompid restarted mid-stream).
    if sps.active().await.unwrap_or(false) {
        claim_source(app, &rc, &client).await;
        if let Ok(md) = rc.metadata().await {
            apply_metadata(app, &md, &mut meta).await;
        }
        if let Ok(state) = rc.player_state().await {
            apply_player_state(app, &rc, &client, &state).await;
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
                        claim_source(app, &rc, &client).await;
                        // Snap the speaker to the sender's slider position.
                        if let Ok(db) = rc.airplay_volume().await {
                            apply_airplay_volume(app, db).await;
                        }
                    }
                    Ok(false) => {
                        live_position = false;
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
                    apply_player_state(app, &rc, &client, &state).await;
                }
            }
            Some(md) = meta_stream.next() => {
                if let Ok(md) = md.get().await {
                    apply_metadata(app, &md, &mut meta).await;
                }
            }
            Some(progress) = progress_stream.next() => {
                // Even new binaries can receive classic prgr without astm.
                // Keep its duration, and its position if MPRIS is unavailable.
                if let Ok(progress) = progress.get().await {
                    let rate = sps.source_format().await.ok()
                        .and_then(|format| source_frame_rate(&format))
                        .unwrap_or(FRAME_RATE);
                    if let Some((position, duration)) = parse_progress(&progress, rate) {
                        apply_progress(app, (!live_position).then_some(position), Some(duration)).await;
                    }
                }
            }
            _ = position_tick.tick(), if plist_metadata => {
                let active = {
                    let s = app.shared.read().await;
                    s.source.active == Some(SourceKind::Airplay) && s.track.is_some()
                };
                if active {
                    match mpris.position().await {
                        Ok(us) => {
                            position_warned = false;
                            let position = position_ms(us);
                            live_position = position.is_some();
                            if position.is_some() {
                                apply_progress(app, position, None).await;
                            }
                        }
                        Err(err) => {
                            live_position = false;
                            if !position_warned {
                                tracing::warn!(%err, "AirPlay MPRIS position unavailable; using native progress when provided");
                                position_warned = true;
                            }
                        }
                    }
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
            Some(a) = available_stream.next() => {
                // Resolve invalidations before reading both capability sources.
                let _ = a.get().await;
                apply_controllable(app, remote_controllable(&rc, &client).await).await;
            }
            Some(commands) = commands_stream.next() => {
                let _ = commands.get().await;
                apply_controllable(app, remote_controllable(&rc, &client).await).await;
            }
            Some(stream_type) = stream_type_stream.next() => {
                live_position = false;
                let _ = stream_type.get().await;
                apply_controllable(app, remote_controllable(&rc, &client).await).await;
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
fn write_config(name: &str, airplay_model: &str, airplay_classic: bool) -> anyhow::Result<()> {
    // RuntimeDirectory= provides /run/boompi under systemd; create it
    // for bench runs launched by hand.
    if let Some(dir) = Path::new(CONF_PATH).parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        CONF_PATH,
        generated_config(name, airplay_model, airplay_classic),
    )?;
    Ok(())
}

fn generated_config(name: &str, airplay_model: &str, airplay_classic: bool) -> String {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    let service_type = if airplay_classic {
        "classic"
    } else {
        "airplay2"
    };
    // Binary-only deploy-dev.sh updates leave patched 4.3.7 installed.
    // Each shairport version ignores the option it does not recognize.
    let legacy_classic_line = if airplay_classic {
        "  airplay_classic_only = \"yes\";\n"
    } else {
        ""
    };
    // Advertised model → the sender's AirPlay-picker icon (patched-in
    // shairport options; see buildroot/patches/shairport-sync). Empty =
    // shairport's default (generic speaker icon). Senders resolve Apple
    // model strings to product icons; anything else needs feature bit 26
    // (ThirdPartySpeaker → bookshelf icon), plus bit 49 for the TV icon
    // (ThirdPartyTV). Apple strings win over the bits, so advertising
    // them only matters for non-Apple models.
    let model_line = if airplay_model.is_empty() {
        String::new()
    } else {
        // Model string only - never feature bits. The third-party icon
        // bits are booby-trapped on current iOS (AirPlay/935.x): bit 26
        // (ThirdPartySpeaker) doubles as Authentication_4, making the
        // sender demand MFi auth-setup that shairport cannot answer
        // (connection aborts); bit 51 draws the icon too but demands
        // HomeKit PIN pairing we do not implement (yet - a panel-
        // displayed pairing code would be the way in). Apple model
        // strings need no bits and handshake fine.
        format!(
            "  airplay_device_model = \"{}\";\n",
            airplay_model.replace('\\', "\\\\").replace('"', "\\\"")
        )
    };
    format!(
        r#"// Generated by boompid - do not edit.
general = {{
  name = "{escaped}";
{model_line}{legacy_classic_line}  service_type = "{service_type}";
  output_backend = "pipe";
  // Don't software-attenuate the PCM: the sender's volume drives the
  // system volume instead (AirplayVolume watcher), matching how AVRCP
  // absolute volume works on the Bluetooth path. Without this the
  // speaker has two volumes in series and the panel slider never moves.
  ignore_volume_control = "yes";
}};
pipe = {{
  name = "{FIFO_PATH}";
  output_rate = 44100;
  output_format = "S16_LE";
  output_channels = 2;
}};
metadata = {{
  enabled = "yes";
  include_cover_art = "yes";
}};
"#
    )
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
        anyhow::bail!(
            "mkfifo {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        );
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

/// FIFO -> `pw-cat --playback` (raw PCM pipe). Shairport 5.x keeps its
/// writer open between sessions; EOF occurs when the receiver exits.
///
/// NB: no `--raw` flag - it doesn't exist before PipeWire 1.4 and makes
/// 1.2.x print usage and exit. Stdin is always treated as a raw pipe
/// whose parameters come from the CLI args, and the rate matters:
/// pw-cat defaults to 48000, which plays 44100 content 8.8% fast
/// (+1.5 semitones).
async fn audio_bridge(fifo: PathBuf) -> anyhow::Result<()> {
    loop {
        // Blocks (on the blocking pool) until a writer appears.
        let mut pipe = tokio::fs::File::open(&fifo).await?;
        let mut pwcat = tokio::process::Command::new("pw-cat")
            .args([
                "--playback",
                "--rate",
                "44100",
                "--channels",
                "2",
                "--format",
                "s16",
                // Tag + route: the mixer steers music streams, and the
                // music bus is where they mix (pre-volume, unity).
                "-P",
                "{ application.name = boompi-music, target.object = music-bus }",
                "-",
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

async fn claim_source(
    app: &SharedApp,
    rc: &RemoteControlProxy<'_>,
    client: &AirplayClientProxy<'_>,
) {
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
        controllable: remote_controllable(rc, client).await,
    };
    let mut s = app.shared.write().await;
    if s.source != source {
        tracing::info!(
            device = ?source.device_name,
            controllable = source.controllable,
            "AirPlay session active"
        );
        s.source = source.clone();
        drop(s);
        app.broadcast(ServerMessage::Source(source));
    }
}

async fn remote_controllable(rc: &RemoteControlProxy<'_>, client: &AirplayClientProxy<'_>) -> bool {
    transport_controllable(
        rc.available().await.unwrap_or(false),
        &rc.stream_type().await.unwrap_or_default(),
        &client.command_information().await.unwrap_or_default(),
    )
}

fn transport_controllable(
    dacp_available: bool,
    stream_type: &str,
    commands: &[OwnedValue],
) -> bool {
    // mrSupportedCommandsFromSender is an array of embedded binary plists.
    // Upstream plist_to_gvariant decodes them into av of a{sv}, with uint64
    // command IDs and booleans. Only commands used by our panel count, and
    // never let a previous AP2 command list enable a classic session.
    dacp_available
        || (matches!(stream_type, "Buffered" | "Realtime")
            && commands.iter().any(|command| {
                let Ok(command) = <HashMap<String, OwnedValue>>::try_from(command.clone()) else {
                    return false;
                };
                let id = command
                    .get("kCommandInfoCommandKey")
                    .and_then(|v| u64::try_from(v).ok());
                let enabled = command
                    .get("kCommandInfoEnabledKey")
                    .and_then(|v| bool::try_from(v).ok());
                matches!(id, Some(0 | 1 | 4 | 5)) && enabled == Some(true)
            }))
}

/// All capability/stream-type updates and source claims use the same calculation.
async fn apply_controllable(app: &SharedApp, available: bool) {
    let mut s = app.shared.write().await;
    if s.source.active == Some(SourceKind::Airplay) && s.source.controllable != available {
        s.source.controllable = available;
        let source = s.source.clone();
        drop(s);
        tracing::info!(available, "AirPlay remote control availability changed");
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

async fn apply_player_state(
    app: &SharedApp,
    rc: &RemoteControlProxy<'_>,
    client: &AirplayClientProxy<'_>,
    state: &str,
) {
    let status = match state {
        "Playing" => PlaybackStatus::Playing,
        "Paused" => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    };
    if status == PlaybackStatus::Playing {
        claim_source(app, rc, client).await;
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
/// previous state - naively rebuilding the track on each burst reset the
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

async fn apply_progress(app: &SharedApp, position_ms: Option<u32>, duration_ms: Option<u32>) {
    let track = {
        let mut s = app.shared.write().await;
        if s.source.active != Some(SourceKind::Airplay) {
            return;
        }
        match s.track.as_mut() {
            Some(track) => {
                if let Some(position) = position_ms {
                    track.position_ms = Some(position);
                    track.updated_at = now_ms();
                }
                if let Some(duration) = duration_ms.filter(|duration| *duration > 0) {
                    track.duration_ms = Some(duration);
                }
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
    let us = i64::try_from(v.clone()).ok().or_else(|| {
        u64::try_from(v.clone())
            .ok()
            .and_then(|u| i64::try_from(u).ok())
    })?;
    if us <= 0 {
        return None;
    }
    u32::try_from(us / 1000).ok()
}

fn source_frame_rate(format: &str) -> Option<u32> {
    // SourceFormat is e.g. "ALAC/48000/S24/2", not the fixed pipe format.
    format
        .split('/')
        .nth(1)?
        .parse()
        .ok()
        .filter(|rate| *rate > 0)
}

fn position_ms(microseconds: i64) -> Option<u32> {
    u32::try_from(u64::try_from(microseconds).ok()? / 1000).ok()
}

/// Legacy progress, with the same signed wrapping RTP differences upstream uses.
fn parse_progress(s: &str, frame_rate: u32) -> Option<(u32, u32)> {
    let mut parts = s.split('/');
    let start: u32 = parts.next()?.trim().parse().ok()?;
    let current: u32 = parts.next()?.trim().parse().ok()?;
    let end: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || frame_rate == 0 {
        return None;
    }
    let position = u64::try_from(current.wrapping_sub(start) as i32).ok()?;
    let duration = u64::try_from(end.wrapping_sub(start) as i32).ok()?;
    if position > duration {
        return None;
    }
    Some((
        u32::try_from(position * 1000 / u64::from(frame_rate)).ok()?,
        u32::try_from(duration * 1000 / u64::from(frame_rate)).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_info(id: u64, enabled: bool) -> OwnedValue {
        OwnedValue::from(HashMap::from([
            ("kCommandInfoCommandKey".to_string(), OwnedValue::from(id)),
            (
                "kCommandInfoEnabledKey".to_string(),
                OwnedValue::from(enabled),
            ),
        ]))
    }

    #[test]
    fn enabled_transport_commands() {
        for id in [0, 1, 4, 5] {
            for stream_type in ["Buffered", "Realtime"] {
                assert!(transport_controllable(
                    false,
                    stream_type,
                    &[command_info(id, true)]
                ));
                assert!(!transport_controllable(
                    false,
                    stream_type,
                    &[command_info(id, false)]
                ));
            }
            for stream_type in ["Classic", "AirPlay", "", "Unknown", "buffered"] {
                assert!(!transport_controllable(
                    false,
                    stream_type,
                    &[command_info(id, true)]
                ));
                assert!(transport_controllable(
                    true,
                    stream_type,
                    &[command_info(id, true)]
                ));
            }
        }
        // PlayPause/Stop/volume/shuffle/seek alone do not implement our buttons.
        for id in [2, 3, 12, 13, 25, 26, 999] {
            assert!(!transport_controllable(
                false,
                "Buffered",
                &[command_info(id, true)]
            ));
        }
        assert!(transport_controllable(
            false,
            "Buffered",
            &[command_info(0, false), command_info(1, true)],
        ));
    }

    #[test]
    fn missing_and_malformed_commands() {
        assert!(!transport_controllable(false, "Buffered", &[]));
        assert!(transport_controllable(true, "", &[])); // Client absent on 4.x
        let malformed = [
            OwnedValue::from(true),
            OwnedValue::from(HashMap::<String, OwnedValue>::new()),
            OwnedValue::from(HashMap::from([(
                "kCommandInfoCommandKey".to_string(),
                OwnedValue::from(0u64),
            )])),
            OwnedValue::from(HashMap::from([(
                "kCommandInfoEnabledKey".to_string(),
                OwnedValue::from(true),
            )])),
            OwnedValue::from(HashMap::from([
                (
                    "kCommandInfoCommandKey".to_string(),
                    OwnedValue::from(zbus::zvariant::Str::from("0")),
                ),
                ("kCommandInfoEnabledKey".to_string(), OwnedValue::from(true)),
            ])),
            OwnedValue::from(HashMap::from([
                ("kCommandInfoCommandKey".to_string(), OwnedValue::from(0u64)),
                ("kCommandInfoEnabledKey".to_string(), OwnedValue::from(1u64)),
            ])),
            // A data item that upstream could not decode remains ay, not a dict.
            OwnedValue::try_from(zbus::zvariant::Value::from(vec![0u8, 1])).unwrap(),
        ];
        for entry in malformed {
            assert!(!transport_controllable(false, "Buffered", &[entry]));
        }
    }

    #[tokio::test]
    async fn capability_updates_combine_both_sources() {
        let app = crate::state::App::new(crate::config::Config::default(), None);
        app.shared.write().await.source.active = Some(SourceKind::Airplay);
        // Initial claim, AP2 enable, DACP changes, AP2 disable, then DACP loss.
        for (dacp, commands, expected) in [
            (false, vec![], false),
            (false, vec![command_info(0, true)], true),
            (true, vec![command_info(0, true)], true),
            (false, vec![command_info(0, true)], true),
            (true, vec![command_info(0, false)], true),
            (false, vec![command_info(0, false)], false),
            (true, vec![], true),
            (false, vec![], false),
        ] {
            apply_controllable(&app, transport_controllable(dacp, "Buffered", &commands)).await;
            assert_eq!(app.shared.read().await.source.controllable, expected);
        }
        let commands = [command_info(0, true)];
        for (stream_type, expected) in [
            ("Buffered", true),
            ("Classic", false),
            ("Realtime", true),
            ("AirPlay", false),
            ("", false),
        ] {
            apply_controllable(&app, transport_controllable(false, stream_type, &commands)).await;
            assert_eq!(app.shared.read().await.source.controllable, expected);
        }
        app.shared.write().await.source.active = Some(SourceKind::Bluetooth);
        apply_controllable(&app, true).await;
        assert!(!app.shared.read().await.source.controllable);
    }

    #[test]
    fn config_fixes_pipe_format_and_selects_service_type() {
        for (classic, service_type) in [(false, "airplay2"), (true, "classic")] {
            let conf = generated_config("BoomPi", "AudioAccessory5,1", classic);
            assert!(conf.contains(&format!("service_type = \"{service_type}\";")));
            assert!(conf.contains("output_backend = \"pipe\";"));
            assert!(conf.contains("ignore_volume_control = \"yes\";"));
            assert!(conf.contains(&format!(
                "pipe = {{\n  name = \"{FIFO_PATH}\";\n  output_rate = 44100;\n  output_format = \"S16_LE\";\n  output_channels = 2;\n}};"
            )));
            assert!(conf.contains("airplay_device_model = \"AudioAccessory5,1\";"));
            assert!(conf.contains("include_cover_art = \"yes\";"));
            assert_eq!(conf.contains("airplay_classic_only = \"yes\";"), classic);
            if !classic {
                assert!(!conf.contains("airplay_classic_only"));
            }
            assert!(!conf.contains("get_plist_metadata"));
        }
        let conf = generated_config("a\"b\\c", "m\"n\\o", false);
        assert!(conf.contains(r#"name = "a\"b\\c";"#));
        assert!(conf.contains(r#"airplay_device_model = "m\"n\\o";"#));
        assert!(!generated_config("BoomPi", "", false).contains("airplay_device_model"));
    }

    #[test]
    fn parses_progress_string() {
        // 60 s track, 15 s in.
        for rate in [44_100, 48_000] {
            for start in [1_000_000u32, u32::MAX - rate] {
                let s = format!(
                    "{start}/{}/{}",
                    start.wrapping_add(15 * rate),
                    start.wrapping_add(60 * rate)
                );
                assert_eq!(parse_progress(&s, rate), Some((15_000, 60_000)));
            }
        }
        assert_eq!(source_frame_rate("ALAC/48000/S24/2"), Some(48_000));
        assert_eq!(source_frame_rate("AAC/44100/F32/2"), Some(FRAME_RATE));
        for format in ["", "48000/S16_LE/2", "ALAC/0/S16/2", "ALAC/no/S16/2"] {
            assert_eq!(source_frame_rate(format), None);
        }
    }

    #[test]
    fn mpris_position_is_time_not_rtp_frames() {
        assert_eq!(position_ms(15_000_999), Some(15_000));
        assert_eq!(position_ms(0), Some(0));
        assert_eq!(position_ms(-1), None);
        assert_eq!(position_ms(i64::MAX), None);
    }

    #[tokio::test]
    async fn native_progress_supplies_duration_without_astm() {
        let app = crate::state::App::new(crate::config::Config::default(), None);
        app.shared.write().await.source.active = Some(SourceKind::Airplay);
        let md = HashMap::from([(
            "xesam:title".to_string(),
            OwnedValue::from(zbus::zvariant::Str::from("Classic track")),
        )]);
        apply_metadata(&app, &md, &mut MetaState::default()).await;
        assert_eq!(
            app.shared.read().await.track.as_ref().unwrap().duration_ms,
            None
        );

        // Works without MPRIS; no astm/mpris:length was supplied.
        let (position, duration) = parse_progress("0/661500/2646000", FRAME_RATE).unwrap();
        apply_progress(&app, Some(position), Some(duration)).await;
        let track = app.shared.read().await.track.clone().unwrap();
        assert_eq!(track.position_ms, Some(15_000));
        assert_eq!(track.duration_ms, Some(60_000));

        // A new binary's live MPRIS position must not erase native duration.
        apply_progress(&app, position_ms(16_000_000), None).await;
        let track = app.shared.read().await.track.clone().unwrap();
        assert_eq!(track.position_ms, Some(16_000));
        assert_eq!(track.duration_ms, Some(60_000));

        // Native timing ahead of audible MPRIS progress supplies only duration.
        let (position, duration) = parse_progress("0/882000/3087000", FRAME_RATE).unwrap();
        let live_position = true;
        apply_progress(&app, (!live_position).then_some(position), Some(duration)).await;
        let updated = app.shared.read().await.track.clone().unwrap();
        assert_eq!(updated.position_ms, Some(16_000));
        assert_eq!(updated.duration_ms, Some(70_000));
        assert_eq!(updated.updated_at, track.updated_at);
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
        for s in [
            "",
            "1/2",
            "a/b/c",
            "100/50/20",
            "1/3/2",
            "1/2/3/4",
            "0/1/18446744073709551615",
        ] {
            assert_eq!(parse_progress(s, FRAME_RATE), None);
        }
        assert_eq!(parse_progress("0/1/2", 0), None);
    }
}
