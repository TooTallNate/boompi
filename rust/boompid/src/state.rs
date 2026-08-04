//! Shared daemon state and client command handling.

use boompi_proto::{
    Battery, BtDevice, BtDeviceAction, ClientMessage, Pairing, PairingAction, PairingState,
    PlaybackStatus, ServerMessage, Settings, SetupState, SourceInfo, SourceKind, State, Track,
};
use bytes::Bytes;
use std::collections::HashMap;
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

/// Transport/volume commands routed to the active hardware source
/// (BlueZ today; librespot/shairport arbitration lands in Phase 3).
/// Only constructed/consumed on Linux (hardware sources are cfg-gated).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub enum SourceCommand {
    Play,
    Pause,
    Next,
    Previous,
    SetVolume(f32),
}

/// Bluetooth control commands routed to the bluetooth task (pairing agent
/// decisions, discoverable toggling, device management).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub enum BtCommand {
    Pairing(PairingAction),
    Device {
        address: String,
        action: BtDeviceAction,
    },
}

pub type SharedApp = Arc<App>;

pub struct App {
    /// Boot-time config. Runtime-mutable values (name, theme, ...) live in
    /// `shared.settings` — read those, not this, for anything a user can
    /// change; `cfg` remains for boot-time facts (battery bus, model, ...).
    pub cfg: crate::config::Config,
    /// Where to persist config changes (None = --config not given).
    config_path: Option<std::path::PathBuf>,
    /// Bumped whenever runtime config changes in a way that requires
    /// sources to re-announce (speaker rename). Sources watch this and
    /// restart their sessions with the fresh name.
    cfg_generation: tokio::sync::watch::Sender<u64>,
    pub started: Instant,
    pub shared: RwLock<Shared>,
    pub tx: broadcast::Sender<Outbound>,
    /// Signalled by Next/Previous; the sim track loop (and later, sources
    /// without native skip) listens on this.
    pub sim_skip: Notify,
    /// Command channels registered by hardware sources (BlueZ, Spotify).
    /// Transport commands route to the *active* source; when no source is
    /// registered the built-in/sim path applies commands directly.
    source_cmds:
        std::sync::Mutex<HashMap<SourceKind, tokio::sync::mpsc::UnboundedSender<SourceCommand>>>,
    /// Bluetooth control channel (pairing + device management), registered
    /// by the bluetooth task when it starts.
    bt_ctl: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<BtCommand>>>,
    /// Small LRU cache of album artwork, keyed by `artwork_id`
    /// (served via `GET /art/{id}` and pushed as binary frames).
    art: RwLock<ArtCache>,
}

// Only populated by the Linux-only artwork worker; harmless elsewhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Default)]
struct ArtCache {
    map: std::collections::HashMap<String, Bytes>,
    order: std::collections::VecDeque<String>,
}

/// AVRCP thumbnails are ~10–30 KB; keep memory bounded regardless.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const ART_CACHE_CAP: usize = 16;

/// Mutable state mirrored to clients (see [`boompi_proto::State`]).
#[derive(Debug, Default)]
pub struct Shared {
    pub source: SourceInfo,
    pub track: Option<Track>,
    pub volume: f32,
    pub battery: Option<Battery>,
    pub pairing: Pairing,
    pub bt_devices: Vec<BtDevice>,
    pub settings: Settings,
    pub setup: SetupState,
    /// Number of clients currently requesting fast battery polling.
    pub fast_poll_clients: usize,
}

impl App {
    pub fn new(cfg: crate::config::Config, config_path: Option<std::path::PathBuf>) -> SharedApp {
        let (tx, _) = broadcast::channel(256);
        let settings = Settings {
            name: cfg.name.clone(),
            theme: cfg.settings.theme,
            online_art_fallback: cfg.settings.online_art_fallback,
        };
        Arc::new(Self {
            cfg,
            config_path,
            cfg_generation: tokio::sync::watch::channel(0).0,
            started: Instant::now(),
            shared: RwLock::new(Shared {
                volume: 0.5,
                settings,
                ..Shared::default()
            }),
            tx,
            sim_skip: Notify::new(),
            source_cmds: std::sync::Mutex::new(HashMap::new()),
            bt_ctl: std::sync::Mutex::new(None),
            art: RwLock::new(ArtCache::default()),
        })
    }

    /// Register the bluetooth control channel.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn register_bt_ctl(&self, tx: tokio::sync::mpsc::UnboundedSender<BtCommand>) {
        *self.bt_ctl.lock().unwrap() = Some(tx);
    }

    fn forward_bt(&self, cmd: BtCommand) -> bool {
        match self.bt_ctl.lock().unwrap().as_ref() {
            Some(tx) => tx.send(cmd).is_ok(),
            None => false,
        }
    }

    /// Current speaker name (runtime truth; may differ from boot config
    /// after a rename).
    pub async fn speaker_name(&self) -> String {
        self.shared.read().await.settings.name.clone()
    }

    /// Subscribe to config-generation bumps (speaker rename → sources
    /// restart their announcements). Only hardware sources listen today.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn subscribe_cfg(&self) -> tokio::sync::watch::Receiver<u64> {
        self.cfg_generation.subscribe()
    }

    /// Persist the current runtime settings back to the config file.
    async fn persist_config(&self) {
        let Some(path) = &self.config_path else {
            tracing::warn!("no --config path; settings change not persisted");
            return;
        };
        let mut cfg = self.cfg.clone();
        {
            let s = self.shared.read().await;
            cfg.name = s.settings.name.clone();
            cfg.settings.theme = s.settings.theme;
            cfg.settings.online_art_fallback = s.settings.online_art_fallback;
        }
        match crate::config::save(&cfg, path) {
            Ok(()) => tracing::info!(path = %path.display(), "config persisted"),
            Err(err) => tracing::error!(%err, path = %path.display(), "config persist failed"),
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub async fn insert_art(&self, id: String, bytes: Bytes) {
        let mut cache = self.art.write().await;
        if !cache.map.contains_key(&id) {
            cache.order.push_back(id.clone());
            if cache.order.len() > ART_CACHE_CAP {
                if let Some(evicted) = cache.order.pop_front() {
                    cache.map.remove(&evicted);
                }
            }
        }
        cache.map.insert(id, bytes);
    }

    pub async fn get_art(&self, id: &str) -> Option<Bytes> {
        self.art.read().await.map.get(id).cloned()
    }

    /// Register a hardware source's command channel (takes over transport
    /// and volume handling from the built-in/sim path).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn register_source(
        &self,
        kind: SourceKind,
        tx: tokio::sync::mpsc::UnboundedSender<SourceCommand>,
    ) {
        self.source_cmds.lock().unwrap().insert(kind, tx);
    }

    /// Route a command to the active source's channel. Volume goes to the
    /// Bluetooth/audio path regardless of source (it owns system volume and
    /// AVRCP sync); transport goes to whoever is actually playing.
    async fn forward_to_source(&self, cmd: SourceCommand) -> bool {
        let target = match cmd {
            SourceCommand::SetVolume(_) => Some(SourceKind::Bluetooth),
            _ => self.shared.read().await.source.active,
        };
        let guard = self.source_cmds.lock().unwrap();
        let tx = target
            .and_then(|kind| guard.get(&kind))
            .or_else(|| guard.get(&SourceKind::Bluetooth));
        match tx {
            Some(tx) => tx.send(cmd).is_ok(),
            None => false,
        }
    }

    pub async fn snapshot(&self) -> State {
        let s = self.shared.read().await;
        State {
            source: s.source.clone(),
            track: s.track.clone(),
            volume: s.volume,
            battery: s.battery.clone(),
            pairing: s.pairing.clone(),
            bt_devices: s.bt_devices.clone(),
            settings: s.settings.clone(),
            setup: s.setup.clone(),
        }
    }

    #[track_caller]
    pub fn broadcast(&self, msg: ServerMessage) {
        // Flow tracing: every Track/Source mutation is attributed to its
        // call site so display-state fights between sources show up
        // directly in the logs.
        let caller = std::panic::Location::caller();
        match &msg {
            ServerMessage::Track(t) => tracing::debug!(
                target: "boompid::flow",
                title = t.title.as_deref().unwrap_or("-"),
                art = t.artwork_id.as_deref().unwrap_or("-"),
                status = ?t.status,
                pos = ?t.position_ms,
                %caller,
                "→ Track"
            ),
            ServerMessage::Source(s) => tracing::info!(
                target: "boompid::flow",
                active = ?s.active,
                device = s.device_name.as_deref().unwrap_or("-"),
                %caller,
                "→ Source"
            ),
            _ => {}
        }
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

    #[track_caller]
    pub fn broadcast_frame(&self, frame: Vec<u8>) {
        if frame.first() == Some(&boompi_proto::frame_tag::ARTWORK) {
            tracing::debug!(
                target: "boompid::flow",
                len = frame.len(),
                caller = %std::panic::Location::caller(),
                "→ artwork frame"
            );
        }
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
            ClientMessage::Play => {
                if !self.forward_to_source(SourceCommand::Play).await {
                    self.set_playback(PlaybackStatus::Playing).await;
                }
            }
            ClientMessage::Pause => {
                if !self.forward_to_source(SourceCommand::Pause).await {
                    self.set_playback(PlaybackStatus::Paused).await;
                }
            }
            ClientMessage::Next => {
                if !self.forward_to_source(SourceCommand::Next).await {
                    self.sim_skip.notify_waiters();
                }
            }
            ClientMessage::Previous => {
                if !self.forward_to_source(SourceCommand::Previous).await {
                    self.sim_skip.notify_waiters();
                }
            }
            ClientMessage::SetVolume { level } => {
                let level = level.clamp(0.0, 1.0);
                if !self
                    .forward_to_source(SourceCommand::SetVolume(level))
                    .await
                {
                    self.shared.write().await.volume = level;
                    self.broadcast(ServerMessage::Volume { level });
                }
            }
            ClientMessage::BatteryFastPoll { .. } => {
                // Handled per-connection in server.rs so a client's fast-poll
                // request is released when it disconnects.
            }
            ClientMessage::Pairing { action } => {
                if !self.forward_bt(BtCommand::Pairing(action)) {
                    // No bluetooth task (--sim / non-Linux): mirror state so
                    // UI development has something to bind to.
                    let mut s = self.shared.write().await;
                    s.pairing = match action {
                        PairingAction::Enable => Pairing {
                            state: PairingState::Discoverable,
                            ..Pairing::default()
                        },
                        PairingAction::Cancel
                        | PairingAction::Reject
                        | PairingAction::Confirm => Pairing::default(),
                    };
                    let pairing = s.pairing.clone();
                    drop(s);
                    self.broadcast(ServerMessage::Pairing(pairing));
                }
            }
            ClientMessage::BtDevice { address, action } => {
                if !self.forward_bt(BtCommand::Device { address, action }) {
                    tracing::info!(?action, "bt device action ignored (no bluetooth task)");
                }
            }
            ClientMessage::SetSettings(patch) => {
                let mut renamed = false;
                let settings = {
                    let mut s = self.shared.write().await;
                    if let Some(v) = patch.online_art_fallback {
                        s.settings.online_art_fallback = v;
                    }
                    if let Some(theme) = patch.theme {
                        s.settings.theme = theme;
                    }
                    if let Some(name) = patch.name {
                        // Sanity limits: BT alias and mDNS instance names
                        // both get unhappy with very long strings.
                        let name = name.trim().chars().take(48).collect::<String>();
                        if !name.is_empty() && name != s.settings.name {
                            tracing::info!(%name, "speaker renamed");
                            s.settings.name = name;
                            renamed = true;
                        }
                    }
                    s.settings.clone()
                };
                self.broadcast(ServerMessage::Settings(settings));
                self.persist_config().await;
                if renamed {
                    // Sources re-announce under the new name (BT alias is
                    // updated in place; AirPlay/Spotify restart discovery).
                    self.cfg_generation.send_modify(|g| *g += 1);
                }
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
