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

/// Read the SoC temperature (°C, one decimal) and live-throttle bit.
/// Linux/Pi paths; `None`/false elsewhere (desktop dev).
pub fn read_diag() -> boompi_proto::DiagState {
    let cpu_temp_c = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|milli| ((milli / 100.0).round() / 10.0) as f32);
    // get_throttled low bits = active conditions (0 under-voltage,
    // 1 arm freq capped, 2 throttled, 3 soft temp limit). Reading the
    // sysfs node clears the sticky (16+) bits, which we don't use.
    let throttled = std::fs::read_to_string("/sys/devices/platform/soc/soc:firmware/get_throttled")
        .ok()
        .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        .map(|bits| bits & 0xF != 0)
        .unwrap_or(false);
    boompi_proto::DiagState { cpu_temp_c, throttled }
}

/// The OS image version stamp: "vX.Y.Z" for release builds,
/// "vX.Y.Z-<sha>" for untagged CI builds (written by the image
/// workflow to /etc/boompi-version), "dev" when absent (local builds,
/// desktop development).
pub fn os_version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        std::fs::read_to_string("/etc/boompi-version")
            .map(|s| s.trim().to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "dev".into())
    })
}

/// Stable per-box identifier: "boompi-" + the last four hex digits of
/// the SoC serial - computed directly rather than read from the
/// hostname, whose file can lag the serial-derived rename on a fresh
/// A/B slot (a boot where that happened re-registered every HA entity
/// under a duplicate device). Falls back to the hostname, then a dev
/// constant. Shared by the MQTT (Home Assistant) device identity and
/// the `_boompi._tcp` mDNS advert's TXT record.
pub fn device_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let serial_id = std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|ci| {
                ci.lines()
                    .find(|l| l.starts_with("Serial"))
                    .and_then(|l| l.split_whitespace().last())
                    .filter(|s| s.len() >= 4)
                    .map(|s| format!("boompi-{}", &s[s.len() - 4..]))
            });
        serial_id
            .or_else(|| {
                std::fs::read_to_string("/etc/hostname")
                    .map(|s| s.trim().to_string())
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "boompi-dev".into())
    })
}

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
    /// `shared.settings` - read those, not this, for anything a user can
    /// change; `cfg` remains for boot-time facts (battery bus, model, ...).
    pub cfg: crate::config::Config,
    /// Where to persist config changes (None = --config not given).
    config_path: Option<std::path::PathBuf>,
    /// Bumped whenever runtime config changes in a way that requires
    /// sources to re-announce (speaker rename). Sources watch this and
    /// restart their sessions with the fresh name.
    cfg_generation: tokio::sync::watch::Sender<u64>,
    /// True while the Bluetooth pairing window is open. The BLE GATT
    /// bridge pauses advertising for the duration: on the UB500,
    /// classic inquiry/discoverable and LE advertising fight over the
    /// radio ("Failed to set mode: Busy") and gamepads can't pair.
    pub pairing_window: tokio::sync::watch::Sender<bool>,
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
    /// Port the settings web UI listener bound (0 = none); set by the
    /// server at startup, read when composing `Hello.settings_url`.
    pub settings_port: std::sync::atomic::AtomicU16,
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

/// AVRCP thumbnails are ~10-30 KB; keep memory bounded regardless.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const ART_CACHE_CAP: usize = 16;

/// Mutable state mirrored to clients (see [`boompi_proto::State`]).
#[derive(Debug, Default)]
pub struct Shared {
    pub source: SourceInfo,
    pub track: Option<Track>,
    /// The user-facing volume: what the sliders show. Equals the sink
    /// volume for locally-scaled sources (Spotify, AirPlay); mirrors the
    /// phone's slider for Bluetooth, where iOS scales the PCM itself.
    /// The music track's volume: one level shared by every audio
    /// source's stream (applied by mixer.rs; the system sink stays at
    /// reference). Remote volume commands (AVRCP, DACP, Spirc) and the
    /// panel/web sliders all read and write this.
    pub volume: f32,
    pub battery: Option<Battery>,
    /// Games library snapshot (maintained by the games module).
    pub games: boompi_proto::GamesState,
    /// "system/file" of the running game.
    pub game_running: Option<String>,
    /// Why telemetry is (not) flowing; UIs explain instead of hiding.
    pub battery_status: boompi_proto::BatteryStatus,
    pub diag: boompi_proto::DiagState,
    pub battery_status_detail: Option<String>,
    pub pairing: Pairing,
    pub bt_devices: Vec<BtDevice>,
    pub settings: Settings,
    pub setup: SetupState,
    /// Wi-Fi link + hotspot state mirrored to clients. Refreshed by the
    /// watcher task in main.rs and after every Wi-Fi action; sim/non-Linux
    /// paths mutate it directly.
    pub wifi: boompi_proto::WifiState,
    /// Durable clock prefs (see config::Config::timezone).
    pub timezone: Option<String>,
    pub ntp: Option<bool>,
    /// Active emoji font id + download-in-flight state (fonts.rs).
    pub emoji_font: String,
    pub emoji_download: Option<String>,
    pub emoji_progress: Option<f32>,
    pub emoji_error: Option<String>,
    /// OS update flow state (update.rs).
    pub update_available: Option<String>,
    pub update_checking: bool,
    pub update_applying: Option<String>,
    pub update_stage: Option<boompi_proto::UpdateStage>,
    pub update_progress: Option<f32>,
    pub update_error: Option<String>,
    /// Number of clients currently requesting fast battery polling.
    pub fast_poll_clients: usize,
}

impl App {
    pub fn new(cfg: crate::config::Config, config_path: Option<std::path::PathBuf>) -> SharedApp {
        let (tx, _) = broadcast::channel(256);
        let settings = Settings {
            name: cfg.name.clone(),
            theme: cfg.settings.theme,
            airplay_model: cfg.settings.airplay_model.clone(),
            ui_scale: cfg.settings.ui_scale,
            update_channel: cfg.settings.update_channel,
            airplay_classic: cfg.settings.airplay_classic,
            clock_24h: cfg.settings.clock_24h,
            game_volume: cfg.settings.game_volume,
            visualizer_opacity: cfg.settings.visualizer_opacity,
            mqtt_broker: cfg.settings.mqtt_broker.clone(),
            mqtt_username: cfg.settings.mqtt_username.clone(),
            mqtt_password: cfg.settings.mqtt_password.clone(),
            screensaver: cfg.settings.screensaver,
            screensaver_min: cfg.settings.screensaver_min,
        };
        let setup = SetupState {
            required: !cfg.setup_complete,
            wifi_status: None,
        };
        // Boot ceiling: restore the persisted volume but never wake
        // louder than 70% - a restart must not be able to blast the
        // house, whatever state was persisted (bench: a calibration
        // session left 1.0 behind; 12:30am; wife).
        let cfg2_volume = cfg.volume.clamp(0.0, 0.7);
        let cfg2_timezone = cfg.timezone.clone();
        let cfg2_ntp = cfg.ntp;
        let cfg2_emoji_font = cfg.settings.emoji_font.clone();
        Arc::new(Self {
            cfg,
            config_path,
            cfg_generation: tokio::sync::watch::channel(0).0,
            pairing_window: tokio::sync::watch::channel(false).0,
            started: Instant::now(),
            shared: RwLock::new(Shared {
                volume: cfg2_volume,
                settings,
                setup,
                timezone: cfg2_timezone,
                ntp: cfg2_ntp,
                emoji_font: cfg2_emoji_font,
                emoji_download: None,
                emoji_progress: None,
                emoji_error: None,
                ..Shared::default()
            }),
            tx,
            sim_skip: Notify::new(),
            source_cmds: std::sync::Mutex::new(HashMap::new()),
            bt_ctl: std::sync::Mutex::new(None),
            settings_port: std::sync::atomic::AtomicU16::new(0),
            art: RwLock::new(ArtCache::default()),
        })
    }

    /// Browser URL for the settings UI, from the LAN IP + bound port.
    /// Recomputed per call - DHCP leases change. With no route to the
    /// internet (onboarding hotspot: NM shared mode, no uplink) fall back
    /// to the AP gateway address.
    pub fn settings_url(&self) -> Option<String> {
        let port = self
            .settings_port
            .load(std::sync::atomic::Ordering::Relaxed);
        if port == 0 {
            return None;
        }
        let ip = lan_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "10.42.0.1".into());
        Some(if port == 80 {
            format!("http://{ip}/")
        } else {
            format!("http://{ip}:{port}/")
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

    /// Mark first-boot setup finished. Deliberately idempotent: when
    /// setup ends over the onboarding hotspot (the finish tap, or a
    /// Wi-Fi join that tears the AP down mid-flight), the client may
    /// never see a response and can retry; re-broadcasting and
    /// re-running the AP teardown are harmless. Returns whether a
    /// pending setup was actually completed (callers persist).
    pub async fn complete_setup(&self) -> bool {
        let was_required = {
            let mut s = self.shared.write().await;
            let was = s.setup.required;
            s.setup.required = false;
            was
        };
        if was_required {
            tracing::info!("first-boot setup completed");
        }
        self.broadcast(ServerMessage::Setup(SetupState::default()));
        #[cfg(target_os = "linux")]
        tokio::spawn(async {
            if let Err(err) = crate::wifi::stop_ap().await {
                tracing::debug!(%err, "onboarding AP teardown (may not be up)");
            }
        });
        was_required
    }

    /// Publish a Wi-Fi join status update (panel setup screen).
    pub async fn set_wifi_status(&self, status: Option<boompi_proto::WifiJoinStatus>) {
        let setup = {
            let mut s = self.shared.write().await;
            s.setup.wifi_status = status;
            s.setup.clone()
        };
        self.broadcast(ServerMessage::Setup(setup));
    }

    /// Re-read Wi-Fi facts from NetworkManager and broadcast when they
    /// changed. Called by the periodic watcher and after every Wi-Fi
    /// action (HTTP or protocol).
    #[cfg(target_os = "linux")]
    pub async fn refresh_wifi(&self) {
        let mut wifi = match crate::wifi::state().await {
            Ok(w) => w,
            Err(err) => {
                tracing::debug!(%err, "wifi state refresh failed");
                return;
            }
        };
        wifi.settings_url = self.settings_url();
        self.publish_wifi(wifi).await;
    }

    /// Store + broadcast a Wi-Fi state snapshot (no-op when unchanged).
    pub async fn publish_wifi(&self, wifi: boompi_proto::WifiState) {
        {
            let mut s = self.shared.write().await;
            if s.wifi == wifi {
                return;
            }
            s.wifi = wifi.clone();
        }
        self.broadcast(ServerMessage::Wifi(wifi));
    }

    /// Persist the current runtime settings back to the config file.
    pub async fn persist_config(&self) {
        let Some(path) = &self.config_path else {
            tracing::warn!("no --config path; settings change not persisted");
            return;
        };
        let mut cfg = self.cfg.clone();
        {
            let s = self.shared.read().await;
            cfg.name = s.settings.name.clone();
            cfg.settings.theme = s.settings.theme;
            cfg.settings.airplay_model = s.settings.airplay_model.clone();
            cfg.settings.ui_scale = s.settings.ui_scale;
            cfg.settings.update_channel = s.settings.update_channel;
            cfg.settings.airplay_classic = s.settings.airplay_classic;
            cfg.settings.clock_24h = s.settings.clock_24h;
            cfg.settings.game_volume = s.settings.game_volume;
            cfg.settings.visualizer_opacity = s.settings.visualizer_opacity;
            cfg.settings.mqtt_broker = s.settings.mqtt_broker.clone();
            cfg.settings.mqtt_username = s.settings.mqtt_username.clone();
            cfg.settings.mqtt_password = s.settings.mqtt_password.clone();
            cfg.settings.screensaver = s.settings.screensaver;
            cfg.settings.screensaver_min = s.settings.screensaver_min;
            cfg.settings.emoji_font = s.emoji_font.clone();
            cfg.setup_complete = !s.setup.required;
            cfg.volume = s.volume;
            cfg.timezone = s.timezone.clone();
            cfg.ntp = s.ntp;
        }
        match crate::config::save(&cfg, path) {
            Ok(()) => tracing::info!(path = %path.display(), "config persisted"),
            Err(err) => tracing::error!(%err, path = %path.display(), "config persist failed"),
        }
    }

    /// Apply a validated speaker rename to shared state. Returns true when
    /// the name actually changed (caller persists + bumps the config
    /// generation so sources re-announce).
    async fn apply_rename(&self, name: String) -> bool {
        // Byte-capped so every advertised identity fits - most
        // restrictively the BLE advert with its emoji prefix
        // (21 bytes; see boompi_proto::ble::SPEAKER_NAME_MAX_BYTES).
        let name = boompi_proto::ble::clamp_speaker_name(&name);
        if name.is_empty() {
            return false;
        }
        let settings = {
            let mut s = self.shared.write().await;
            if name == s.settings.name {
                return false;
            }
            tracing::info!(%name, "speaker renamed");
            s.settings.name = name;
            s.settings.clone()
        };
        self.broadcast(ServerMessage::Settings(settings));
        true
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

    /// A source reported the sender-side volume (AVRCP absolute
    /// volume, AirPlay DACP, Spotify Connect): it becomes the music
    /// track's volume. The mixer applies it to every music stream
    /// within a second; the sink never moves. No-ops (echoes of our
    /// own writes bouncing back) are dropped without a broadcast.
    #[cfg(target_os = "linux")]
    pub async fn apply_external_volume(&self, level: f32) {
        let level = level.clamp(0.0, 1.0);
        {
            let mut s = self.shared.write().await;
            if (s.volume - level).abs() < 0.004 {
                return;
            }
            s.volume = level;
        }
        self.broadcast(ServerMessage::Volume { level });
    }

    /// Route a command to the active source's channel. Volume goes to the
    /// Bluetooth/audio path regardless of source (it owns system volume and
    /// AVRCP sync) - and *additionally* to the active non-Bluetooth source,
    /// so the sender's own slider follows (AirPlay DACP `SetAirplayVolume`,
    /// Spotify Connect `Spirc::set_volume`); transport goes to whoever is
    /// actually playing.
    async fn forward_to_source(&self, cmd: SourceCommand) -> bool {
        let active = self.shared.read().await.source.active;
        let target = match cmd {
            SourceCommand::SetVolume(_) => Some(SourceKind::Bluetooth),
            _ => active,
        };
        let guard = self.source_cmds.lock().unwrap();
        if let SourceCommand::SetVolume(level) = cmd {
            if let Some(kind) = active.filter(|k| *k != SourceKind::Bluetooth) {
                if let Some(tx) = guard.get(&kind) {
                    let _ = tx.send(SourceCommand::SetVolume(level));
                }
            }
        }
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
            games: s.games.clone(),
            battery_status: s.battery_status,
            diag: s.diag.clone(),
            battery_status_detail: s.battery_status_detail.clone(),
            pairing: s.pairing.clone(),
            bt_devices: s.bt_devices.clone(),
            settings: s.settings.clone(),
            setup: s.setup.clone(),
            wifi: s.wifi.clone(),
            emoji_fonts: boompi_proto::EmojiFontsState {
                #[cfg(target_os = "linux")]
                fonts: crate::fonts::list(&s.emoji_font),
                #[cfg(not(target_os = "linux"))]
                fonts: Vec::new(),
                downloading: s.emoji_download.clone(),
                progress: s.emoji_progress,
                error: s.emoji_error.clone(),
            },
            updates: boompi_proto::UpdateState {
                version: os_version().to_string(),
                available: s.update_available.clone(),
                checking: s.update_checking,
                applying: s.update_applying.clone(),
                stage: s.update_stage,
                progress: s.update_progress,
                error: s.update_error.clone(),
            },
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
    pub async fn handle_client_message(self: &std::sync::Arc<Self>, msg: ClientMessage) {
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
            ClientMessage::SetTime { epoch_ms } => {
                #[cfg(target_os = "linux")]
                match crate::clock::offer_time(epoch_ms).await {
                    Ok(true) => {}
                    Ok(false) => tracing::debug!(epoch_ms, "client time offer not needed"),
                    Err(err) => tracing::warn!(%err, epoch_ms, "client time offer rejected"),
                }
                #[cfg(not(target_os = "linux"))]
                tracing::debug!(epoch_ms, "client time offer ignored (non-linux)");
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
                        PairingAction::Cancel | PairingAction::Reject | PairingAction::Confirm => {
                            Pairing::default()
                        }
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
            ClientMessage::EmojiFont { action, id } => {
                #[cfg(target_os = "linux")]
                if let Err(err) = crate::fonts::perform(self, action, &id).await {
                    tracing::warn!(%err, ?action, %id, "emoji font action failed");
                    self.shared.write().await.emoji_error = Some(err.to_string());
                    let snapshot = crate::fonts::state(self).await;
                    self.broadcast(ServerMessage::EmojiFonts(snapshot));
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = (action, id);
                }
            }
            ClientMessage::Update { action } => {
                #[cfg(target_os = "linux")]
                if let Err(err) = crate::update::perform(self, action).await {
                    tracing::warn!(%err, ?action, "update action failed");
                    self.shared.write().await.update_error = Some(format!("{err:#}"));
                    let snapshot = crate::update::state(self).await;
                    self.broadcast(ServerMessage::Update(snapshot));
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = action;
                }
            }
            ClientMessage::SetSettings(patch) => {
                let mut airplay_model_changed = false;
                let mut mqtt_changed = false;
                #[allow(unused_mut, unused_variables)]
                let mut channel_changed = false;
                let settings = {
                    let mut s = self.shared.write().await;
                    if let Some(theme) = patch.theme {
                        s.settings.theme = theme;
                    }
                    if let Some(model) = patch.airplay_model {
                        let model = model.trim().to_string();
                        airplay_model_changed = model != s.settings.airplay_model;
                        s.settings.airplay_model = model;
                    }
                    if let Some(scale) = patch.ui_scale {
                        s.settings.ui_scale = scale.clamp(1.0, 2.5);
                    }
                    if let Some(v) = patch.game_volume {
                        s.settings.game_volume = v.clamp(0.0, 1.0);
                    }
                    if let Some(v) = patch.visualizer_opacity {
                        s.settings.visualizer_opacity = v.clamp(0.1, 1.0);
                    }
                    if let Some(classic) = patch.airplay_classic {
                        if classic != s.settings.airplay_classic {
                            s.settings.airplay_classic = classic;
                            // Same restart trigger as a model change:
                            // the shairport conf embeds the mode.
                            airplay_model_changed = true;
                        }
                    }
                    if let Some(v) = patch.clock_24h {
                        s.settings.clock_24h = v;
                    }
                    if let Some(v) = patch.mqtt_broker {
                        let v = v.trim().to_string();
                        if v != s.settings.mqtt_broker {
                            s.settings.mqtt_broker = v;
                            mqtt_changed = true;
                        }
                    }
                    if let Some(v) = patch.mqtt_username {
                        if v != s.settings.mqtt_username {
                            s.settings.mqtt_username = v;
                            mqtt_changed = true;
                        }
                    }
                    if let Some(v) = patch.mqtt_password {
                        if v != s.settings.mqtt_password {
                            s.settings.mqtt_password = v;
                            mqtt_changed = true;
                        }
                    }
                    if let Some(kind) = patch.screensaver {
                        s.settings.screensaver = kind;
                    }
                    if let Some(min) = patch.screensaver_min {
                        s.settings.screensaver_min = min.clamp(1, 240);
                    }
                    if let Some(channel) = patch.update_channel {
                        if channel != s.settings.update_channel {
                            s.settings.update_channel = channel;
                            // A stale offer from the other channel is
                            // meaningless; the next check refreshes it.
                            s.update_available = None;
                            s.update_error = None;
                            channel_changed = true;
                        }
                    }
                    s.settings.clone()
                };
                self.broadcast(ServerMessage::Settings(settings));
                let renamed = match patch.name {
                    Some(name) => self.apply_rename(name).await,
                    None => false,
                };
                self.persist_config().await;
                if renamed || airplay_model_changed || mqtt_changed {
                    // Sources re-announce under the new name/model (BT
                    // alias is updated in place; AirPlay/Spotify restart
                    // discovery - the AirPlay conf embeds the model).
                    self.cfg_generation.send_modify(|g| *g += 1);
                }
                // Switching channels re-checks immediately (also pushes
                // the cleared `available` to clients).
                #[cfg(target_os = "linux")]
                if channel_changed {
                    let _ = crate::update::perform(self, boompi_proto::UpdateAction::Check).await;
                }
            }
            ClientMessage::Setup(cmd) => {
                let renamed = match cmd.speaker_name {
                    Some(name) => self.apply_rename(name).await,
                    None => false,
                };
                let was_required = if cmd.complete == Some(true) {
                    self.complete_setup().await
                } else {
                    false
                };
                if renamed || was_required {
                    self.persist_config().await;
                }
                if renamed {
                    self.cfg_generation.send_modify(|g| *g += 1);
                }
            }
            ClientMessage::Wifi(action) => {
                #[cfg(target_os = "linux")]
                {
                    // Spawned: joins can take tens of seconds and this is
                    // called from the per-connection WebSocket loop.
                    let app = self.clone();
                    tokio::spawn(async move {
                        use boompi_proto::{WifiAction as W, WifiJoinStatus};
                        let result = match &action {
                            W::Scan => match crate::wifi::status(true).await {
                                Ok(st) => {
                                    app.broadcast(ServerMessage::WifiNetworks {
                                        networks: st.networks,
                                    });
                                    Ok(())
                                }
                                Err(err) => Err(err),
                            },
                            // Full join with password - the path BLE-only
                            // clients use. Progress via WifiJoinStatus like
                            // the HTTP handler (a join over the hotspot
                            // kills the very link that requested it, so
                            // status must survive on the panel/broadcasts).
                            W::Connect { ssid, psk } => {
                                app.set_wifi_status(Some(WifiJoinStatus::Joining {
                                    ssid: ssid.clone(),
                                }))
                                .await;
                                let res = crate::wifi::connect(ssid, psk.as_deref()).await;
                                app.set_wifi_status(Some(match &res {
                                    Ok(()) => WifiJoinStatus::Joined { ssid: ssid.clone() },
                                    Err(err) => WifiJoinStatus::Failed {
                                        ssid: ssid.clone(),
                                        reason: err.to_string(),
                                    },
                                }))
                                .await;
                                res
                            }
                            W::Rejoin { ssid } => crate::wifi::connect(ssid, None).await,
                            W::Disconnect => crate::wifi::disconnect().await,
                            W::Forget { ssid } => crate::wifi::forget(ssid).await,
                            W::Radio { enabled } => crate::wifi::set_radio(*enabled).await,
                            W::Ap { enabled: true } => {
                                crate::wifi::start_ap(&app.speaker_name().await).await
                            }
                            W::Ap { enabled: false } => crate::wifi::stop_ap().await,
                        };
                        if let Err(err) = result {
                            tracing::warn!(%err, ?action, "wifi action failed");
                        }
                        app.refresh_wifi().await;
                    });
                }
                #[cfg(not(target_os = "linux"))]
                {
                    // No NetworkManager (--sim / non-Linux): mirror state
                    // so UI development has something to bind to.
                    use boompi_proto::WifiAction as W;
                    let mut wifi = self.shared.read().await.wifi.clone();
                    match action {
                        W::Scan => {
                            self.broadcast(ServerMessage::WifiNetworks {
                                networks: vec![
                                    boompi_proto::WifiNetwork {
                                        ssid: "Simnet".into(),
                                        signal: 82,
                                        security: "WPA2".into(),
                                        in_use: wifi.connected.as_deref() == Some("Simnet"),
                                        saved: true,
                                    },
                                    boompi_proto::WifiNetwork {
                                        ssid: "Coffee Shop".into(),
                                        signal: 47,
                                        security: "".into(),
                                        in_use: false,
                                        saved: false,
                                    },
                                    boompi_proto::WifiNetwork {
                                        ssid: "Neighbor 5G".into(),
                                        signal: 23,
                                        security: "WPA3".into(),
                                        in_use: false,
                                        saved: false,
                                    },
                                ],
                            });
                            return;
                        }
                        W::Connect { ssid, .. } | W::Rejoin { ssid } => {
                            wifi.connected = Some(ssid);
                            wifi.ip = Some("192.168.1.42/24".into());
                            wifi.ap_active = false;
                            wifi.ap_ssid = None;
                        }
                        W::Disconnect => {
                            wifi.connected = None;
                            wifi.ip = None;
                        }
                        W::Radio { enabled } => {
                            wifi.enabled = enabled;
                            if !enabled {
                                wifi.connected = None;
                                wifi.ip = None;
                            }
                        }
                        W::Forget { ssid } => {
                            wifi.saved.retain(|s| s != &ssid);
                            if wifi.connected.as_deref() == Some(ssid.as_str()) {
                                wifi.connected = None;
                                wifi.ip = None;
                            }
                        }
                        W::Ap { enabled } => {
                            wifi.ap_active = enabled;
                            wifi.ap_ssid = if enabled {
                                Some(self.speaker_name().await)
                            } else {
                                None
                            };
                            if enabled {
                                wifi.connected = None;
                                wifi.ip = Some("10.42.0.1/24".into());
                                wifi.settings_url = Some("http://10.42.0.1/".into());
                            } else {
                                wifi.ip = None;
                                wifi.settings_url = self.settings_url();
                            }
                        }
                    }
                    self.publish_wifi(wifi).await;
                }
            }
            ClientMessage::PreviewScreensaver => {
                self.broadcast(ServerMessage::ScreensaverPreview);
            }
            ClientMessage::Game(action) => {
                let result = match action {
                    boompi_proto::GameAction::Launch { system, file } => {
                        crate::games::launch(self, &system, &file).await
                    }
                    boompi_proto::GameAction::Stop => crate::games::stop().await,
                };
                if let Err(err) = result {
                    tracing::warn!(%err, "game action failed");
                }
            }
            ClientMessage::Reboot => {
                tracing::warn!("reboot requested from a settings UI");
                #[cfg(target_os = "linux")]
                {
                    let _ = tokio::process::Command::new("systemctl")
                        .arg("reboot")
                        .spawn();
                }
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

/// Best-effort LAN IP: the source address the kernel would route to a
/// public host (no packet is sent by `connect` on UDP).
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

/// Hardware model from the device tree ("Raspberry Pi 4 Model B Rev
/// 1.2"); `None` off-device (mac dev, sim on a laptop). The DT string
/// carries a trailing NUL.
pub fn board_model() -> Option<String> {
    let raw = std::fs::read_to_string("/proc/device-tree/model").ok()?;
    let model = raw.trim_matches('\0').trim().to_string();
    (!model.is_empty()).then_some(model)
}

/// Current unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
