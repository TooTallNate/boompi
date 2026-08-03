//! Shared daemon state and client command handling.

use boompi_proto::{
    Battery, ClientMessage, Pairing, PairingAction, PairingState, PlaybackStatus, ServerMessage,
    Settings, SetupState, SourceInfo, State, Track,
};
use bytes::Bytes;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Notify, RwLock};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Anything fanned out to connected WebSocket clients. Payloads are
/// pre-encoded once at broadcast time so the per-subscriber clone is cheap.
#[derive(Debug, Clone)]
pub enum Outbound {
    /// JSON-serialized [`ServerMessage`] text frame.
    Message(Arc<str>),
    /// Binary frame (e.g. visualizer bars).
    Frame(Bytes),
}

pub type SharedApp = Arc<App>;

pub struct App {
    pub cfg: crate::config::Config,
    pub started: Instant,
    pub shared: RwLock<Shared>,
    pub tx: broadcast::Sender<Outbound>,
    /// Signalled by Next/Previous; the sim track loop (and later, sources
    /// without native skip) listens on this.
    pub sim_skip: Notify,
}

/// Mutable state mirrored to clients (see [`boompi_proto::State`]).
#[derive(Debug, Default)]
pub struct Shared {
    pub source: SourceInfo,
    pub track: Option<Track>,
    pub volume: f32,
    pub battery: Option<Battery>,
    pub pairing: Pairing,
    pub settings: Settings,
    pub setup: SetupState,
    /// Number of clients currently requesting fast battery polling.
    pub fast_poll_clients: usize,
}

impl App {
    pub fn new(cfg: crate::config::Config) -> SharedApp {
        let (tx, _) = broadcast::channel(256);
        let settings = Settings {
            online_art_fallback: cfg.settings.online_art_fallback,
        };
        Arc::new(Self {
            cfg,
            started: Instant::now(),
            shared: RwLock::new(Shared {
                volume: 0.5,
                settings,
                ..Shared::default()
            }),
            tx,
            sim_skip: Notify::new(),
        })
    }

    pub async fn snapshot(&self) -> State {
        let s = self.shared.read().await;
        State {
            source: s.source.clone(),
            track: s.track.clone(),
            volume: s.volume,
            battery: s.battery.clone(),
            pairing: s.pairing.clone(),
            settings: s.settings.clone(),
            setup: s.setup.clone(),
        }
    }

    pub fn broadcast(&self, msg: ServerMessage) {
        let json = match serde_json::to_string(&msg) {
            Ok(json) => json,
            Err(err) => {
                tracing::error!(%err, "failed to serialize broadcast");
                return;
            }
        };
        // Errors just mean no clients are connected.
        let _ = self.tx.send(Outbound::Message(json.into()));
    }

    pub fn broadcast_frame(&self, frame: Vec<u8>) {
        let _ = self.tx.send(Outbound::Frame(Bytes::from(frame)));
    }

    /// Handle a command from a client.
    ///
    /// NOTE: currently acts directly on shared state, which is exactly right
    /// for `--sim`; in Phase 1 transport/volume commands are forwarded to the
    /// active source (BlueZ `MediaPlayer1`, librespot, ...) and state changes
    /// flow back from source events instead.
    pub async fn handle_client_message(&self, msg: ClientMessage) {
        tracing::debug!(?msg, "client message");
        match msg {
            ClientMessage::Play => self.set_playback(PlaybackStatus::Playing).await,
            ClientMessage::Pause => self.set_playback(PlaybackStatus::Paused).await,
            ClientMessage::Next | ClientMessage::Previous => {
                // Sim treats both as "skip to next".
                self.sim_skip.notify_waiters();
            }
            ClientMessage::SetVolume { level } => {
                let level = level.clamp(0.0, 1.0);
                self.shared.write().await.volume = level;
                // TODO(Phase 1): set PipeWire sink volume + AVRCP absolute
                // volume on the connected device.
                self.broadcast(ServerMessage::Volume { level });
            }
            ClientMessage::BatteryFastPoll { .. } => {
                // Handled per-connection in server.rs so a client's fast-poll
                // request is released when it disconnects.
            }
            ClientMessage::Pairing { action } => {
                // TODO(Phase 3): drive BlueZ Adapter1 discoverable +
                // Agent1 confirm/reject. For now just mirror state so UI
                // development has something to bind to.
                let mut s = self.shared.write().await;
                s.pairing = match action {
                    PairingAction::Enable => Pairing {
                        state: PairingState::Discoverable,
                        ..Pairing::default()
                    },
                    PairingAction::Cancel | PairingAction::Reject | PairingAction::Confirm => {
                        Pairing::default()
                    }
                };
                let pairing = s.pairing.clone();
                drop(s);
                self.broadcast(ServerMessage::Pairing(pairing));
            }
            ClientMessage::SetSettings(patch) => {
                let mut s = self.shared.write().await;
                if let Some(v) = patch.online_art_fallback {
                    s.settings.online_art_fallback = v;
                }
                if let Some(name) = patch.name {
                    // TODO(Phase 3): update BT alias + persist to config.
                    tracing::info!(%name, "speaker rename requested (not yet persisted)");
                }
                let settings = s.settings.clone();
                drop(s);
                // TODO(Phase 3): persist settings to /data/boompi.toml.
                self.broadcast(ServerMessage::Settings(settings));
            }
            ClientMessage::Setup(cmd) => {
                tracing::info!(?cmd, "setup command (Phase 5)");
            }
        }
    }

    /// Adjust the fast-poll refcount (called by the connection handler).
    pub async fn set_fast_poll(&self, delta: isize) {
        let mut s = self.shared.write().await;
        s.fast_poll_clients = s.fast_poll_clients.saturating_add_signed(delta);
        tracing::debug!(clients = s.fast_poll_clients, "battery fast-poll refcount");
    }

    async fn set_playback(&self, status: PlaybackStatus) {
        let mut s = self.shared.write().await;
        let Some(track) = s.track.as_mut() else {
            return;
        };
        // Snapshot the interpolated position before changing status so
        // clients keep an accurate baseline.
        let now = now_ms();
        if track.status == PlaybackStatus::Playing {
            let elapsed = now.saturating_sub(track.updated_at) as u32;
            track.position_ms = Some(
                track
                    .position_ms
                    .unwrap_or(0)
                    .saturating_add(elapsed)
                    .min(track.duration_ms.unwrap_or(u32::MAX)),
            );
        }
        track.status = status;
        track.updated_at = now;
        let track = track.clone();
        drop(s);
        self.broadcast(ServerMessage::Track(track));
    }
}

/// Current unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
