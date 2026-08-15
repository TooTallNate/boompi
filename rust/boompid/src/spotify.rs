//! Spotify Connect source - librespot embedded as a library.
//!
//! Embedding (vs the earlier subprocess + event-hook approach) gives us:
//! - a `Spirc` handle, so the panel's transport buttons actually control
//!   Spotify (play/pause/next/prev)
//! - in-process `PlayerEvent`s - no hook process, no HTTP, no missed skips
//! - credentials cached across restarts (Spotify app pairs once)
//!
//! Audio goes through a custom [`Sink`] that pipes raw PCM into
//! `pw-cat --playback --raw` (the plain `pw-play FILE` path runs libsndfile,
//! which rejects headerless PCM). PipeWire mixing means the visualizer sees
//! this source for free.

#![cfg(target_os = "linux")]

use crate::state::{now_ms, SharedApp, SourceCommand};
use boompi_proto::{PlaybackStatus, ServerMessage, SourceInfo, SourceKind, Track};
use futures_util::StreamExt;
use librespot::connect::{ConnectConfig, Spirc};
use librespot::core::{cache::Cache, config::DeviceType, Session, SessionConfig};
use librespot::discovery::Discovery;
use librespot::metadata::audio::UniqueFields;
use librespot::playback::audio_backend::{Sink, SinkError, SinkResult};
use librespot::playback::config::{Bitrate, PlayerConfig};
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use librespot::playback::mixer::{self, MixerConfig, NoOpVolume};
use librespot::playback::player::{Player, PlayerEvent};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::mpsc;

pub fn spawn(app: SharedApp) {
    if !app.cfg.spotify.enabled {
        tracing::info!("Spotify Connect disabled by config");
        return;
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    app.register_source(SourceKind::Spotify, tx);
    tokio::spawn(async move {
        loop {
            // Clean exits (rename, fresh discovery credentials) restart
            // almost immediately; failures back off.
            match run_once(&app, &mut rx).await {
                Ok(()) => tokio::time::sleep(Duration::from_secs(1)).await,
                Err(err) => {
                    tracing::warn!(%err, "spotify source failed; restarting in 10s");
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
    let mut cfg_watch = app.subscribe_cfg();
    cfg_watch.mark_unchanged();
    let cache_dir = cache_dir();
    tokio::fs::create_dir_all(&cache_dir).await.ok();
    // Credentials + remembered volume persist; no audio cache (SD wear).
    let cache = Cache::new(Some(&cache_dir), Some(&cache_dir), None, None)?;

    let session_config = SessionConfig {
        device_id: stable_device_id(&name),
        ..SessionConfig::default()
    };

    // Pick the zeroconf backend per run: avahi owns UDP :5353 whenever the
    // daemon is around (AirPlay brings it in), and running libmdns next to
    // it ends in 'responder died' panics. Re-probed on every restart.
    let backend_name = if avahi_present().await {
        "avahi"
    } else {
        "libmdns"
    };
    tracing::info!(backend = backend_name, "spotify zeroconf backend");
    let backend = librespot::discovery::find(Some(backend_name))
        .map_err(|e| anyhow::anyhow!("zeroconf backend {backend_name}: {e}"))?;

    let mut discovery = Discovery::builder(
        session_config.device_id.clone(),
        session_config.client_id.clone(),
    )
    .name(name.clone())
    .device_type(DeviceType::Speaker)
    .zeroconf_backend(backend)
    .launch()?;

    let credentials = match cache.credentials() {
        Some(creds) => creds,
        None => {
            tracing::info!("no cached Spotify credentials; waiting for the app to pick us");
            discovery
                .next()
                .await
                .ok_or_else(|| anyhow::anyhow!("discovery stream ended"))?
        }
    };

    let session = Session::new(session_config, Some(cache));
    let mixer = mixer::find(None).ok_or_else(|| anyhow::anyhow!("no softvol mixer"))?(
        MixerConfig::default(),
    )?;
    let player_config = PlayerConfig {
        bitrate: Bitrate::Bitrate320,
        ..PlayerConfig::default()
    };
    // NoOpVolume: don't software-attenuate the PCM. The mixer still tracks
    // the Connect device volume (and Spirc reports it to the app), but the
    // audible volume is the *system* volume, driven from VolumeChanged
    // events below - parity with the Bluetooth (AVRCP) and AirPlay (DACP)
    // paths, so the phone app, the panel slider and the web UI all agree.
    let player = Player::new(player_config, session.clone(), Box::new(NoOpVolume), || {
        Box::new(PwCatSink::default())
    });
    let mut events = player.get_player_event_channel();

    let connect_config = ConnectConfig {
        name: name.clone(),
        device_type: DeviceType::Speaker,
        ..ConnectConfig::default()
    };
    let (spirc, spirc_task) =
        Spirc::new(connect_config, session, credentials, player, mixer).await?;
    tokio::pin!(spirc_task);
    tracing::info!(%name, "Spotify Connect active (librespot embedded)");

    loop {
        tokio::select! {
            _ = &mut spirc_task => {
                clear_if_active(app).await;
                anyhow::bail!("spirc task ended");
            }
            _ = cfg_watch.changed() => {
                tracing::info!("speaker renamed; restarting Spotify Connect");
                let _ = spirc.shutdown();
                clear_if_active(app).await;
                return Ok(());
            }
            creds = discovery.next() => {
                // Some(_): a (possibly different) account tapped us while a
                // session is live; credentials are cached on connect, so a
                // restart picks the freshest state up cleanly. None: the
                // zeroconf backend died - restart re-probes avahi/libmdns.
                match creds {
                    Some(_) => tracing::info!("new discovery credentials; restarting session"),
                    None => tracing::warn!("discovery stream ended (zeroconf error?); restarting session"),
                }
                let _ = spirc.shutdown();
                clear_if_active(app).await;
                return Ok(());
            }
            ev = events.recv() => match ev {
                Some(ev) => handle_player_event(app, ev).await,
                None => {
                    clear_if_active(app).await;
                    anyhow::bail!("player event channel closed");
                }
            },
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { anyhow::bail!("command channel closed") };
                let result = match cmd {
                    SourceCommand::Play => spirc.play(),
                    SourceCommand::Pause => spirc.pause(),
                    SourceCommand::Next => spirc.next(),
                    SourceCommand::Previous => spirc.prev(),
                    // The bluetooth/audio path already set the system
                    // volume; this makes the Spotify app's slider follow.
                    SourceCommand::SetVolume(level) => {
                        spirc.set_volume((level.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16)
                    }
                };
                if let Err(err) = result {
                    tracing::warn!(%err, ?cmd, "spirc command failed");
                }
            }
        }
    }
}

async fn handle_player_event(app: &SharedApp, ev: PlayerEvent) {
    match ev {
        PlayerEvent::TrackChanged { audio_item } => {
            claim_source(app, None).await;
            let (artist, album) = match &audio_item.unique_fields {
                UniqueFields::Track { artists, album, .. } => (
                    Some(
                        artists
                            .0
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    Some(album.clone()),
                ),
                _ => (None, None),
            };
            let track = Track {
                title: Some(audio_item.name.clone()),
                artist,
                album,
                duration_ms: Some(audio_item.duration_ms),
                position_ms: Some(0),
                status: current_status(app).await,
                artwork_id: None,
                updated_at: now_ms(),
            };
            app.shared.write().await.track = Some(track.clone());
            app.broadcast(ServerMessage::Track(track));

            // Largest cover for the panel (full-size beats BT's 200×200).
            if let Some(cover) = audio_item.covers.iter().max_by_key(|c| c.width) {
                fetch_cover(app.clone(), cover.url.clone());
            }
        }
        PlayerEvent::Playing { position_ms, .. } => {
            claim_source(app, None).await;
            update_status(app, PlaybackStatus::Playing, Some(position_ms)).await;
        }
        PlayerEvent::Paused { position_ms, .. } => {
            update_status(app, PlaybackStatus::Paused, Some(position_ms)).await;
        }
        PlayerEvent::Stopped { .. } => {
            update_status(app, PlaybackStatus::Stopped, None).await;
        }
        PlayerEvent::SessionConnected { user_name, .. } => {
            tracing::info!(%user_name, "Spotify session connected");
            claim_source(app, Some(user_name)).await;
        }
        PlayerEvent::SessionDisconnected { .. } => {
            tracing::info!("Spotify session disconnected");
            clear_if_active(app).await;
        }
        PlayerEvent::VolumeChanged { volume } => {
            // Connect device volume (0..=65535) → system volume. `None`
            // is allowed so a pre-playback volume drag still lands; other
            // active sources must not be yanked around by the app.
            let active = app.shared.read().await.source.active;
            if matches!(active, Some(SourceKind::Spotify) | None) {
                let level = f32::from(volume) / f32::from(u16::MAX);
                tracing::debug!(volume, level, "spotify connect volume changed");
                app.apply_external_volume(level).await;
            }
        }
        _ => {}
    }
}

async fn claim_source(app: &SharedApp, user: Option<String>) {
    let mut s = app.shared.write().await;
    let device_name = match (user, &s.source) {
        (Some(user), _) => user,
        (None, src) if src.active == Some(SourceKind::Spotify) => {
            src.device_name.clone().unwrap_or_else(|| "Spotify".into())
        }
        _ => "Spotify".into(),
    };
    let source = SourceInfo {
        active: Some(SourceKind::Spotify),
        device_name: Some(device_name),
        controllable: true,
    };
    if s.source != source {
        s.source = source.clone();
        drop(s);
        app.broadcast(ServerMessage::Source(source));
    }
}

async fn clear_if_active(app: &SharedApp) {
    let mut s = app.shared.write().await;
    if s.source.active == Some(SourceKind::Spotify) {
        s.source = SourceInfo::default();
        s.track = None;
        drop(s);
        app.broadcast(ServerMessage::Source(SourceInfo::default()));
    }
}

async fn update_status(app: &SharedApp, status: PlaybackStatus, position_ms: Option<u32>) {
    let track = {
        let mut s = app.shared.write().await;
        match s.track.as_mut() {
            Some(track) => {
                track.status = status;
                if position_ms.is_some() {
                    track.position_ms = position_ms;
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

async fn current_status(app: &SharedApp) -> PlaybackStatus {
    app.shared
        .read()
        .await
        .track
        .as_ref()
        .map(|t| t.status)
        .unwrap_or(PlaybackStatus::Playing)
}

/// Download the cover image from Spotify's CDN and publish it.
fn fetch_cover(app: SharedApp, url: String) {
    tokio::spawn(async move {
        match reqwest::get(&url).await.and_then(|r| r.error_for_status()) {
            Ok(response) => match response.bytes().await {
                Ok(bytes) => {
                    tracing::info!(size = bytes.len(), "spotify cover fetched");
                    crate::artwork::publish_current_art(&app, bytes, SourceKind::Spotify).await;
                }
                Err(err) => tracing::warn!(%err, "spotify cover read failed"),
            },
            Err(err) => tracing::warn!(%err, %url, "spotify cover fetch failed"),
        }
    });
}

/// True when avahi-daemon owns its well-known name on the system bus.
async fn avahi_present() -> bool {
    let Ok(conn) = zbus::Connection::system().await else {
        return false;
    };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
        return false;
    };
    let Ok(name) = "org.freedesktop.Avahi".try_into() else {
        return false;
    };
    dbus.name_has_owner(name).await.unwrap_or(false)
}

fn cache_dir() -> String {
    std::env::var("XDG_CACHE_HOME")
        .map(|c| format!("{c}/boompi-spotify"))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{h}/.cache/boompi-spotify"))
                .unwrap_or_else(|_| "/var/cache/boompi-spotify".into())
        })
}

/// Stable across restarts so the Spotify app recognizes the device.
fn stable_device_id(name: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    format!("boompi{:032x}", hasher.finish() as u128)
}

// ---------------------------------------------------------------------------
// Audio sink: s16le PCM → `pw-cat --playback` (raw pipe) → PipeWire default
// sink. No `--raw` flag (absent before PipeWire 1.4; 1.2.x exits with
// usage), and the explicit --rate is load-bearing: stdin is a raw pipe
// whose parameters come from the CLI, default rate 48000 ≠ 44100.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PwCatSink {
    child: Option<std::process::Child>,
}

impl PwCatSink {
    fn stdin(&mut self) -> SinkResult<&mut std::process::ChildStdin> {
        self.child
            .as_mut()
            .and_then(|c| c.stdin.as_mut())
            .ok_or_else(|| SinkError::NotConnected("pw-cat not running".into()))
    }
}

impl Sink for PwCatSink {
    fn start(&mut self) -> SinkResult<()> {
        if self.child.is_none() {
            let child = std::process::Command::new("pw-cat")
                .args([
                    "--playback",
                    "--rate",
                    "44100",
                    "--channels",
                    "2",
                    "--format",
                    "s16",
                    // Tag the stream so the mixer can steer the music
                    // track (bare pw-cat streams are indistinguishable
                    // from each other in pw-dump).
                    "-P",
                    "{ application.name = boompi-music }",
                    "-",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| SinkError::ConnectionRefused(format!("pw-cat spawn: {e}")))?;
            self.child = Some(child);
        }
        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        if let Some(mut child) = self.child.take() {
            drop(child.stdin.take()); // EOF → pw-cat drains and exits
            let _ = child.wait();
        }
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let AudioPacket::Samples(samples) = packet else {
            return Ok(());
        };
        let s16 = converter.f64_to_s16(&samples);
        let mut bytes = Vec::with_capacity(s16.len() * 2);
        for sample in &s16 {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        if let Err(e) = self.stdin()?.write_all(&bytes) {
            // pw-cat died (e.g. pipewire restart): drop it, next start() respawns.
            self.child = None;
            return Err(SinkError::OnWrite(format!("pw-cat write: {e}")));
        }
        Ok(())
    }
}
