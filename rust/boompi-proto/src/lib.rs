//! Boompi protocol v2: message types shared between `boompid` and clients.
//!
//! Transport is a WebSocket (default port 3001, path `/ws`):
//!
//! - **Text frames** carry JSON-encoded [`ServerMessage`] / [`ClientMessage`]
//!   envelopes, internally tagged via a `"type"` field.
//! - **Binary frames** carry high-rate data and are identified by their first
//!   byte (see [`frame_tag`]). Currently only visualizer bars.
//!
//! On connect, the server sends [`ServerMessage::Hello`] followed by a full
//! [`ServerMessage::State`] snapshot, then deltas as things change.
//!
//! Artwork is *not* sent over the WebSocket: [`Track::artwork_id`] identifies
//! an image retrievable via `GET /art/{id}` on the same host/port.

use serde::{Deserialize, Serialize};

/// Protocol version, sent in [`Hello`]. Bump on breaking changes.
pub const PROTO_VERSION: u32 = 2;

/// Default TCP port for the boompid WebSocket/HTTP server.
pub const DEFAULT_PORT: u16 = 3001;

/// First-byte tags for binary WebSocket frames.
pub mod frame_tag {
    /// Visualizer bars: payload is N little-endian `u16` values
    /// (full scale = `u16::MAX`).
    pub const VISUALIZER: u8 = 0x01;
    /// Album artwork for the current track: payload is an encoded image
    /// (JPEG/PNG bytes, typically the AVRCP 200×200 thumbnail). Sent on
    /// track/artwork changes and after the `state` snapshot on connect.
    /// The same bytes are available via `GET /art/{artwork_id}`.
    pub const ARTWORK: u8 = 0x02;
}

// ---------------------------------------------------------------------------
// Common types
// ---------------------------------------------------------------------------

/// Which audio source is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Bluetooth,
    Spotify,
    Airplay,
}

/// Playback status, normalized across sources.
/// (BlueZ `forward-seek`/`reverse-seek` map to the seek variants.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
    ForwardSeek,
    ReverseSeek,
    Error,
}

/// The active source (if any) and the human-readable device/account name
/// associated with it (e.g. the phone's Bluetooth alias).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub active: Option<SourceKind>,
    pub device_name: Option<String>,
    /// Whether transport commands (play/pause/next/previous) can reach
    /// the sender. Bluetooth (AVRCP) and Spotify always can; AirPlay
    /// depends on the sender running a DACP server, which modern iOS
    /// does not do for AirPlay 2 sessions - the panel dims its
    /// transport controls when this is false.
    #[serde(default = "default_true")]
    pub controllable: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SourceInfo {
    fn default() -> Self {
        Self {
            active: None,
            device_name: None,
            controllable: true,
        }
    }
}

/// Track metadata. `position_ms` is a snapshot taken at `updated_at`;
/// clients interpolate while `status == Playing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u32>,
    pub position_ms: Option<u32>,
    pub status: PlaybackStatus,
    /// Fetch via `GET /art/{artwork_id}` when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_id: Option<String>,
    /// Unix timestamp (milliseconds) at which this snapshot was taken.
    pub updated_at: u64,
}

/// Battery telemetry (INA260). Not all boxes have one; absent when
/// unequipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    /// Volts.
    pub voltage: f32,
    /// Amps; negative while charging.
    pub current: f32,
    /// Watts.
    pub power: f32,
    /// 0.0-1.0 state of charge. Coulomb-counted once a full charge has
    /// anchored the estimator; voltage-mapped against the learned full
    /// voltage until then. Exactly 1.0 only when `full`.
    pub percentage: f32,
    pub charging: bool,
    /// Charge termination detected: the charger is holding the pack at
    /// its CV plateau with ~zero current.
    #[serde(default)]
    pub full: bool,
    /// Low-battery warning (hysteresis; clears while charging).
    #[serde(default)]
    pub low: bool,
    /// Estimated time to empty. Present only while discharging with a
    /// learned pack capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_remaining_secs: Option<u32>,
    /// Unix timestamp (milliseconds).
    pub ts: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingState {
    #[default]
    Idle,
    /// Adapter is discoverable, waiting for a device to initiate pairing.
    Discoverable,
    /// A device requested pairing; awaiting on-screen confirmation.
    Confirm,
    /// Pairing in progress with no decision to make (gamepad autopair).
    /// Distinct from Confirm so the UI never flashes Pair/Reject
    /// buttons for a question nobody is being asked.
    Pairing,
    /// No Bluetooth adapter present (dongle unplugged / bluetoothd down).
    /// Broadcast instead of silently ignoring a pairing request - a dead
    /// button is indistinguishable from a bug.
    Unavailable,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pairing {
    pub state: PairingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passkey: Option<u32>,
}

/// A Bluetooth device known to the adapter (paired, or mid-pairing).
///
/// (The per-device volume-mode assignment is gone: every sender is
/// handled the AVRCP-spec way - full-scale PCM in, the speaker
/// renders the volume. iOS's source-side scaling turned out to be a
/// reaction to the host's hw-volume handshake, disabled in the
/// wireplumber config.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BtDevice {
    /// Colon-form address ("6C:3A:FF:58:84:4C") - the id for device actions.
    pub address: String,
    pub name: String,
    pub connected: bool,
    /// Coarse device class for grouping in UIs, from BlueZ's Icon
    /// classification: "phone" | "controller" | "computer" | "audio"
    /// | "other". Old boxes omit it.
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BtDeviceAction {
    Connect,
    Disconnect,
    /// Unpair (BlueZ `RemoveDevice`).
    Remove,
}

/// UI theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

fn default_ui_scale() -> f32 {
    1.0
}

/// Idle screensaver style (mostly-black, slowly moving content: the
/// panels showed burn-in after long static idle).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreensaverKind {
    Off,
    /// Big drifting clock.
    #[default]
    Clock,
    /// Matrix digital rain.
    Matrix,
    /// Drifting album art of the last-played track.
    Art,
}

fn default_screensaver_min() -> u32 {
    10
}

/// Which releases the software updater follows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// Tagged releases (vX.Y.Z).
    #[default]
    Stable,
    /// The rolling "edge" prerelease: every green build of the dev
    /// branch.
    Edge,
}

/// User-adjustable settings, mirrored to all clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Speaker name: Bluetooth alias + AirPlay receiver + Spotify Connect
    /// device, all at once.
    #[serde(default)]
    pub name: String,
    /// Panel UI theme.
    #[serde(default)]
    pub theme: Theme,
    /// When a source provides no album art, look it up online
    /// (iTunes Search / Cover Art Archive) by artist+album.
    pub online_art_fallback: bool,
    /// Advertised AirPlay device model (mDNS `am=`/`model=`): senders pick
    /// their AirPlay-picker icon from it. Empty = shairport default
    /// (generic speaker). E.g. "AudioAccessory5,1" shows a HomePod mini.
    #[serde(default)]
    pub airplay_model: String,
    /// Panel UI scale factor (1.0 = design size). The panel rescales
    /// live; small high-DPI screens (HyperPixel) ship larger defaults.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// Which releases the software updater follows.
    #[serde(default)]
    pub update_channel: UpdateChannel,
    /// Advertise/serve classic AirPlay only (no AirPlay 2): trades
    /// multi-room away for a working remote-control channel - modern
    /// iOS runs no DACP server for AirPlay 2 sessions, so the
    /// speaker's own transport buttons only work on classic.
    #[serde(default)]
    pub airplay_classic: bool,
    /// 24-hour clock (footer + screensaver); 12-hour with AM/PM when
    /// false.
    #[serde(default)]
    pub clock_24h: bool,
    /// The game track's loudness (0.0-1.0): RetroArch's stream volume,
    /// independent of the music track. No ducking - each track holds
    /// its own level and the system sink stays at reference.
    #[serde(default = "default_game_volume")]
    pub game_volume: f32,
    /// Background visualizer opacity on the panel (0.1-1.0).
    #[serde(default = "default_visualizer_opacity")]
    pub visualizer_opacity: f32,
    /// MQTT broker for Home Assistant integration ("host" or
    /// "host:port"; empty = disabled). Entities appear in HA via MQTT
    /// discovery.
    #[serde(default)]
    pub mqtt_broker: String,
    #[serde(default)]
    pub mqtt_username: String,
    #[serde(default)]
    pub mqtt_password: String,
    /// Idle screensaver style (shown after `screensaver_min` minutes
    /// without touches while nothing is playing).
    #[serde(default)]
    pub screensaver: ScreensaverKind,
    /// Idle minutes before the screensaver starts.
    #[serde(default = "default_screensaver_min")]
    pub screensaver_min: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: String::new(),
            theme: Theme::default(),
            online_art_fallback: false,
            airplay_model: String::new(),
            ui_scale: default_ui_scale(),
            update_channel: UpdateChannel::default(),
            airplay_classic: false,
            clock_24h: false,
            game_volume: default_game_volume(),
            visualizer_opacity: default_visualizer_opacity(),
            mqtt_broker: String::new(),
            mqtt_username: String::new(),
            mqtt_password: String::new(),
            screensaver: ScreensaverKind::default(),
            screensaver_min: default_screensaver_min(),
        }
    }
}

/// Partial settings update; `None` fields are left unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online_art_fallback: Option<bool>,
    /// Rename the speaker (Bluetooth alias + AirPlay + Spotify Connect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airplay_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_channel: Option<UpdateChannel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airplay_classic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_24h: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_volume: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_broker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mqtt_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screensaver: Option<ScreensaverKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screensaver_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visualizer_opacity: Option<f32>,
}

/// Wi-Fi link + hotspot state, mirrored to all clients (panel Wi-Fi
/// card, web settings). Scan results are *not* included - scanning is
/// on-demand ([`WifiAction::Scan`] or `GET /api/wifi`); this carries
/// only the cheap always-known facts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WifiState {
    /// A Wi-Fi capable device exists.
    pub supported: bool,
    /// Radio on?
    pub enabled: bool,
    /// SSID of the active connection, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<String>,
    /// wlan IP when connected or in AP mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    /// The speaker's own hotspot (open AP) is broadcasting.
    pub ap_active: bool,
    /// SSID the hotspot broadcasts while `ap_active`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ap_ssid: Option<String>,
    /// Saved Wi-Fi profile names (rejoinable without a password).
    #[serde(default)]
    pub saved: Vec<String>,
    /// Browser URL of the settings web UI reachable *right now* (LAN
    /// address, or the hotspot gateway while `ap_active`). The panel
    /// re-renders its QR code from this - `Hello.settings_url` only
    /// arrives on connect and goes stale when the hotspot toggles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_url: Option<String>,
}

/// One scanned Wi-Fi network (deduped by SSID, strongest kept).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0-100.
    pub signal: u8,
    /// nmcli security string ("WPA2", ...); "" = open.
    pub security: String,
    pub in_use: bool,
    pub saved: bool,
}

/// Wi-Fi actions available over the protocol (panel, web, BLE). The
/// full lifecycle rides the protocol so BLE-only clients (the hosted
/// remote, phone apps) can manage Wi-Fi with no IP path at all:
/// [`WifiAction::Scan`] answers with a [`ServerMessage::WifiNetworks`]
/// broadcast, [`WifiAction::Connect`] carries the password. The HTTP
/// API (`POST /api/wifi`) remains as the synchronous-error flavor the
/// box's own web app prefers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum WifiAction {
    /// Scan for nearby networks; results arrive as a
    /// [`ServerMessage::WifiNetworks`] broadcast.
    Scan,
    /// Join a network. `psk: None` for open networks (or saved
    /// profiles, though [`WifiAction::Rejoin`] says that clearer).
    /// Join progress is surfaced via `WifiJoinStatus` and the
    /// follow-up `Wifi` state broadcast.
    Connect {
        ssid: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        psk: Option<String>,
    },
    /// Reconnect a saved network (profile keeps its password).
    Rejoin { ssid: String },
    /// Drop the current connection (suppresses autoconnect until a
    /// manual rejoin - the "leave my home Wi-Fi" camping prep).
    Disconnect,
    /// Delete a saved profile.
    Forget { ssid: String },
    /// Radio on/off.
    Radio { enabled: bool },
    /// The speaker's own hotspot: phones join it to reach the web UI
    /// (and this WebSocket) with no shared network - camping mode.
    Ap { enabled: bool },
}

/// Live Wi-Fi join progress, surfaced on the panel: the join usually
/// kills the portal connection it was requested over (single radio),
/// so the speaker's own screen is the only reliable status display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WifiJoinStatus {
    Joining { ssid: String },
    Joined { ssid: String },
    Failed { ssid: String, reason: String },
}

/// Box health diagnostics (CPU thermal state). Broadcast periodically
/// and on meaningful change; previously MQTT-only, which made Home
/// Assistant the only place to see the temperature.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiagState {
    /// SoC temperature in °C (one decimal), absent off-hardware.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_temp_c: Option<f32>,
    /// The firmware is actively limiting the clock right now
    /// (under-voltage or soft thermal limit - the "why is it slow"
    /// bit that once cost a bench session to discover).
    #[serde(default)]
    pub throttled: bool,
}

/// First-boot setup state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupState {
    /// True until initial configuration has been completed. While set, the
    /// panel shows the onboarding screen and (when Wi-Fi hardware exists
    /// and nothing is connected) the speaker broadcasts its own hotspot.
    pub required: bool,
    /// Most recent Wi-Fi join attempt (cleared when setup completes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wifi_status: Option<WifiJoinStatus>,
}

/// First-boot setup commands (sent by the web wizard).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupCommand {
    /// Set the speaker name (required before completing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_name: Option<String>,
    /// Finish setup: clears `required`, persists, and tears the
    /// onboarding hotspot down. Wi-Fi is optional - completing without
    /// ever configuring it is fine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingAction {
    /// Make the speaker discoverable/pairable.
    Enable,
    /// Leave discoverable mode / abort an in-flight request.
    Cancel,
    /// Accept the pending pairing request.
    Confirm,
    /// Reject the pending pairing request.
    Reject,
}

// ---------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------

/// Sent by the server on connect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub proto_version: u32,
    /// Speaker name (Bluetooth alias / pretty hostname).
    pub name: String,
    /// Hardware model, read from the device tree at runtime
    /// (e.g. "Raspberry Pi 4 Model B Rev 1.2"); absent off-device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// boompid version.
    pub version: String,
    pub uptime_secs: u64,
    /// Browser URL of the settings web UI (LAN address), when known.
    /// The panel renders this as a QR code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_url: Option<String>,
    /// Feature flags for UIs that outlive any given box's software
    /// (hosted remote, phone apps): what this box can actually do.
    /// Clients hide features whose capability is absent. Unknown
    /// strings must be ignored (future boxes will grow more). An old
    /// box that predates this field sends nothing - clients fall back
    /// to the legacy feature set (see [`caps::LEGACY`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

/// Capability names carried in [`Hello::capabilities`].
pub mod caps {
    /// Wi-Fi hardware exists (scan/join/hotspot make sense at all).
    pub const WIFI: &str = "wifi";
    /// Wi-Fi scans + password joins ride the protocol
    /// ([`crate::WifiAction::Scan`]/`Connect`) - absent on boxes that
    /// predate them, where Wi-Fi management needs the REST API.
    pub const WIFI_SCAN: &str = "wifi_scan";
    /// A battery monitor is configured (hard-wired boxes drop this).
    pub const BATTERY: &str = "battery";
    /// A Bluetooth adapter is present (pairing/devices UI).
    pub const BLUETOOTH: &str = "bluetooth";
    /// Game library + emulator.
    pub const GAMES: &str = "games";
    pub const EMOJI_FONTS: &str = "emoji_fonts";
    pub const UPDATES: &str = "updates";
    pub const SCREENSAVER: &str = "screensaver";
    pub const HOME_ASSISTANT: &str = "home_assistant";
    pub const AIRPLAY: &str = "airplay";

    /// What a box that predates the capabilities field supports:
    /// everything except the protocol Wi-Fi lifecycle (its Wi-Fi
    /// management was REST-only). Clients use this when
    /// `Hello.capabilities` is empty.
    pub const LEGACY: &[&str] = &[
        WIFI,
        BATTERY,
        BLUETOOTH,
        GAMES,
        EMOJI_FONTS,
        UPDATES,
        SCREENSAVER,
        HOME_ASSISTANT,
        AIRPLAY,
    ];
}

/// One emoji font in the selection catalog (see boompid's fonts.rs).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmojiFontInfo {
    pub id: String,
    pub label: String,
    pub license: String,
    pub installed: bool,
    pub active: bool,
    pub builtin: bool,
    /// Download size in bytes (0 for the built-in).
    pub size: u64,
}

/// Emoji font catalog + download state, mirrored to all clients.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmojiFontsState {
    pub fonts: Vec<EmojiFontInfo>,
    /// Font id currently downloading, if any.
    pub downloading: Option<String>,
    /// Download progress 0.0-1.0 while `downloading` is set.
    pub progress: Option<f32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmojiFontAction {
    Download,
    Select,
    Remove,
}

/// What the updater is doing right now while `applying` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    /// Streaming the system image into the inactive slot.
    DownloadingSystem,
    /// Re-reading the written system image against its checksum.
    VerifyingSystem,
    /// Streaming the boot files into the inactive slot.
    DownloadingBoot,
    /// Re-reading the written boot files against their checksum.
    VerifyingBoot,
    /// Staged and verified; arming the trial boot + restarting.
    Restarting,
}

/// OS software update state, mirrored to all clients.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateState {
    /// Version of the running OS image (/etc/boompi-version), e.g.
    /// "v2.0.0" for a tagged release or "v2.0.0-abcdefg" for an
    /// untagged build. "dev" when not running a CI image.
    pub version: String,
    /// Newer version available on the selected channel, if any.
    pub available: Option<String>,
    /// A release check is in flight.
    pub checking: bool,
    /// Version currently being downloaded + staged, if any.
    pub applying: Option<String>,
    /// Current step while `applying` is set.
    pub stage: Option<UpdateStage>,
    /// Progress 0.0-1.0 while `applying` is set (download, write and
    /// verify phases combined). The box reboots into the update trial
    /// when it reaches the end.
    pub progress: Option<f32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAction {
    /// Query the release channel now.
    Check,
    /// Download + stage the available update, then reboot into the
    /// trial boot.
    Apply,
}

/// Why battery telemetry is (or is not) flowing, so UIs can explain
/// an absent battery instead of silently hiding it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryStatus {
    /// No `[battery]` section configured for this box.
    #[default]
    Unconfigured,
    /// Configured but the sensor is not responding (wiring, wrong
    /// bus/address); detail in `State::battery_status_detail`.
    Error,
    /// Telemetry active.
    Ok,
}

/// Full state snapshot; sent after [`Hello`] and available on demand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub source: SourceInfo,
    pub track: Option<Track>,
    pub volume: f32,
    pub battery: Option<Battery>,
    #[serde(default)]
    pub games: GamesState,
    #[serde(default)]
    pub battery_status: BatteryStatus,
    #[serde(default)]
    pub diag: DiagState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_status_detail: Option<String>,
    pub pairing: Pairing,
    /// Paired Bluetooth devices.
    #[serde(default)]
    pub bt_devices: Vec<BtDevice>,
    pub settings: Settings,
    pub setup: SetupState,
    #[serde(default)]
    pub wifi: WifiState,
    #[serde(default)]
    pub emoji_fonts: EmojiFontsState,
    #[serde(default)]
    pub updates: UpdateState,
}

fn default_visualizer_opacity() -> f32 {
    1.0
}

fn default_game_volume() -> f32 {
    0.5
}

/// A playable ROM in the on-box library (/data/games/roms/<system>/).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub system: String,
    /// File name within the system directory.
    pub file: String,
    /// Display name (file name without extension).
    pub name: String,
    pub size: u64,
}

/// Games library + runtime state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GamesState {
    pub games: Vec<Game>,
    /// "system/file" of the running game, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<String>,
    /// A gamepad is connected; launching requires one (the panel's
    /// touch input dies while RetroArch owns the display).
    pub gamepad: bool,
    /// /data free/total bytes (the ROM library lives there).
    pub storage_free: u64,
    pub storage_total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum GameAction {
    /// Launch a game: the panel UI stops (one DRM master at a time),
    /// RetroArch runs, the panel returns on exit.
    Launch { system: String, file: String },
    /// Stop the running game (the no-gamepad escape hatch).
    Stop,
}

/// Server → client messages (JSON text frames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello(Hello),
    State(State),
    Source(SourceInfo),
    Track(Track),
    Volume {
        level: f32,
    },
    Battery(Battery),
    Games(GamesState),
    Pairing(Pairing),
    // NB: struct form - internally-tagged serde can't represent a newtype
    // variant wrapping a sequence.
    BtDevices {
        devices: Vec<BtDevice>,
    },
    Settings(Settings),
    Setup(SetupState),
    Wifi(WifiState),
    /// Scan results, answering a [`WifiAction::Scan`] (broadcast to
    /// all clients - scans are radio-global anyway).
    WifiNetworks {
        networks: Vec<WifiNetwork>,
    },
    /// Box health diagnostics (CPU temperature / throttle state).
    Diag(DiagState),
    EmojiFonts(EmojiFontsState),
    Update(UpdateState),
    /// Relay: a client asked to preview the screensaver; the panel
    /// activates it immediately.
    ScreensaverPreview,
    /// The box is about to power itself off (e.g. battery empty). The
    /// panel shows a full-screen notice for the grace period.
    PowerOff {
        reason: String,
        in_secs: u32,
    },
}

/// Client → server messages (JSON text frames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Play,
    Pause,
    /// Manage emoji fonts (download/select/remove by catalog id).
    EmojiFont {
        action: EmojiFontAction,
        id: String,
    },
    /// OS software update (check the channel / apply the available
    /// update).
    Update {
        action: UpdateAction,
    },
    Next,
    Previous,
    SetVolume {
        level: f32,
    },
    /// While any client has this enabled, battery telemetry is polled and
    /// broadcast at ~1 Hz instead of the slow default.
    BatteryFastPoll {
        enabled: bool,
    },
    Pairing {
        action: PairingAction,
    },
    /// Manage a known Bluetooth device.
    BtDevice {
        address: String,
        action: BtDeviceAction,
    },
    SetSettings(SettingsPatch),
    Setup(SetupCommand),
    /// Wi-Fi management (rejoin/disconnect/forget/hotspot).
    Wifi(WifiAction),
    /// Preview the configured screensaver on the panel right now
    /// (relayed to the panel via [`ServerMessage::ScreensaverPreview`]).
    PreviewScreensaver,
    /// Orderly reboot (settings UIs; also how a box-profile change
    /// takes effect).
    Reboot,
    /// Games (launch/stop).
    Game(GameAction),
    /// Offer the client's wall-clock time as a fallback sync source.
    ///
    /// The boxes have no RTC, so without internet (NTP) the clock is
    /// wildly wrong. Clients that know the time (browsers, phone apps)
    /// send this on connect; the server applies it only when NTP has
    /// not synchronized, so a reachable NTP server always wins.
    SetTime {
        /// Unix time in milliseconds (`Date.now()`).
        epoch_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// Binary frame helpers
// ---------------------------------------------------------------------------

/// Encode visualizer bars as a tagged binary frame.
pub fn encode_visualizer_frame(bars: &[u16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + bars.len() * 2);
    buf.push(frame_tag::VISUALIZER);
    for bar in bars {
        buf.extend_from_slice(&bar.to_le_bytes());
    }
    buf
}

/// Encode current-track artwork (encoded image bytes) as a tagged frame.
pub fn encode_artwork_frame(image: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + image.len());
    buf.push(frame_tag::ARTWORK);
    buf.extend_from_slice(image);
    buf
}

/// Extract the image payload from an artwork frame (including its tag byte).
pub fn decode_artwork_frame(frame: &[u8]) -> Option<&[u8]> {
    match frame.split_first() {
        Some((&frame_tag::ARTWORK, payload)) => Some(payload),
        _ => None,
    }
}

/// Decode a visualizer binary frame (including its tag byte).
/// Returns `None` if the tag or length is wrong.
pub fn decode_visualizer_frame(frame: &[u8]) -> Option<Vec<u16>> {
    let (&tag, payload) = frame.split_first()?;
    if tag != frame_tag::VISUALIZER || payload.len() % 2 != 0 {
        return None;
    }
    Some(
        payload
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// BLE GATT transport (see docs/BLE.md)
// ---------------------------------------------------------------------------

/// BLE GATT control channel: the same JSON [`ServerMessage`] /
/// [`ClientMessage`] envelopes as the WebSocket, carried over a custom
/// GATT service so phones (native apps via CoreBluetooth/Android BLE,
/// or Chrome via Web Bluetooth) can control the speaker with **no
/// shared IP network at all**.
///
/// JSON messages routinely exceed the ATT MTU (~23-517 bytes), so both
/// characteristics carry *chunked* messages: each chunk starts with a
/// 1-byte header of [`ble::CHUNK_FIRST`] / [`ble::CHUNK_LAST`] flags.
/// High-rate binary frames (visualizer, artwork) are deliberately NOT
/// carried over BLE - fetch artwork via `GET /art/{id}` when an IP
/// path exists.
pub mod ble {
    /// Primary GATT service advertised by boompid.
    pub const SERVICE_UUID: &str = "a5e90001-9c60-4b2a-a6ca-0d0a2b5f0e1f";
    /// Write / write-without-response: client → server chunked JSON
    /// [`super::ClientMessage`].
    pub const CONTROL_CHAR_UUID: &str = "a5e90002-9c60-4b2a-a6ca-0d0a2b5f0e1f";
    /// Notify: server → client chunked JSON [`super::ServerMessage`].
    /// Subscribing greets the client with `hello` + a full `state`
    /// snapshot (like a WebSocket connect), then streams deltas.
    pub const EVENTS_CHAR_UUID: &str = "a5e90003-9c60-4b2a-a6ca-0d0a2b5f0e1f";
    /// Read: full JSON [`super::State`] snapshot (GATT long-read;
    /// offset reads continue the snapshot taken at offset 0). An
    /// alternative to the subscription greeting for on-demand polls.
    pub const STATE_CHAR_UUID: &str = "a5e90004-9c60-4b2a-a6ca-0d0a2b5f0e1f";

    /// The GATT advert's name prefix: distinguishes the control
    /// channel from the A2DP entry in phones' Bluetooth lists (the
    /// car-key pattern). U+1F39B needs VS16 to reliably render as
    /// emoji rather than a monochrome dial.
    pub const ADVERT_PREFIX: &str = "\u{1F39B}\u{FE0F} ";
    /// Legacy advertising caps the scan-response name at 29 bytes;
    /// BlueZ rejects oversized registrations outright.
    pub const ADVERT_NAME_MAX: usize = 29;
    /// Max speaker-name bytes such that the advert always fits.
    pub const SPEAKER_NAME_MAX_BYTES: usize = ADVERT_NAME_MAX - ADVERT_PREFIX.len();

    /// Trim + byte-cap a speaker name so every advertised identity
    /// (BLE advert incl. prefix, BT alias, mDNS instance) fits.
    /// Char-boundary safe: a multi-byte emoji never gets split.
    pub fn clamp_speaker_name(name: &str) -> String {
        let trimmed = name.trim();
        if trimmed.len() <= SPEAKER_NAME_MAX_BYTES {
            return trimmed.to_string();
        }
        let mut end = SPEAKER_NAME_MAX_BYTES;
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        trimmed[..end].trim_end().to_string()
    }

    /// Chunk header flag: first chunk of a message (resets reassembly).
    pub const CHUNK_FIRST: u8 = 0x01;
    /// Chunk header flag: last chunk of a message (message complete).
    pub const CHUNK_LAST: u8 = 0x02;

    /// Reassembly cap: protocol messages are small; anything bigger is
    /// a framing error, not a message.
    pub const MAX_MESSAGE: usize = 64 * 1024;

    /// Conservative default chunk size (header + payload) when the ATT
    /// MTU is unknown: fits the 185-byte MTU iOS negotiates by default
    /// (notification payload = MTU - 3).
    pub const DEFAULT_CHUNK: usize = 176;

    /// Split a message into tagged chunks of at most `max_chunk` bytes
    /// (header byte included). `max_chunk` values below 2 (header +
    /// one payload byte) are clamped to 2 so the output never violates
    /// the size bound.
    pub fn chunk_message(payload: &[u8], max_chunk: usize) -> Vec<Vec<u8>> {
        let body = max_chunk.max(2) - 1;
        let mut chunks: Vec<Vec<u8>> = payload
            .chunks(body)
            .map(|c| {
                let mut buf = Vec::with_capacity(1 + c.len());
                buf.push(0);
                buf.extend_from_slice(c);
                buf
            })
            .collect();
        if chunks.is_empty() {
            chunks.push(vec![0]); // empty message: one empty chunk
        }
        chunks.first_mut().unwrap()[0] |= CHUNK_FIRST;
        chunks.last_mut().unwrap()[0] |= CHUNK_LAST;
        chunks
    }

    /// Reassembles chunked messages. One instance per client/direction.
    #[derive(Debug, Default)]
    pub struct Reassembler {
        buf: Vec<u8>,
        /// A FIRST chunk has arrived and the message is still open.
        open: bool,
    }

    impl Reassembler {
        /// Feed one chunk; returns the complete message when its LAST
        /// chunk lands. Malformed sequences (missing FIRST, oversize)
        /// drop the partial message and resynchronize on the next
        /// FIRST chunk.
        pub fn push(&mut self, chunk: &[u8]) -> Option<Vec<u8>> {
            let (&flags, payload) = chunk.split_first()?;
            if flags & CHUNK_FIRST != 0 {
                self.buf.clear();
                self.open = true;
            } else if !self.open {
                return None; // continuation without a start: drop
            }
            if self.buf.len() + payload.len() > MAX_MESSAGE {
                self.buf.clear();
                self.open = false;
                return None;
            }
            self.buf.extend_from_slice(payload);
            if flags & CHUNK_LAST != 0 {
                self.open = false;
                return Some(std::mem::take(&mut self.buf));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_message_json_shape() {
        let msg = ServerMessage::Track(Track {
            title: Some("Song 2".into()),
            artist: Some("Blur".into()),
            album: None,
            duration_ms: Some(122_000),
            position_ms: Some(0),
            status: PlaybackStatus::Playing,
            artwork_id: None,
            updated_at: 1_700_000_000_000,
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "track");
        assert_eq!(json["status"], "playing");
        assert!(json.get("artwork_id").is_none());
        let back: ServerMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn speaker_name_clamp() {
        use super::ble;
        assert_eq!(ble::SPEAKER_NAME_MAX_BYTES, 21);
        assert_eq!(ble::clamp_speaker_name("George\u{2019}s \u{1F50A}"), "George\u{2019}s \u{1F50A}"); // 15 bytes
        // 20 ASCII + a 4-byte emoji would be 24: emoji dropped whole.
        assert_eq!(ble::clamp_speaker_name("aaaaaaaaaaaaaaaaaaaa\u{1F50A}"), "aaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            format!("{}{}", ble::ADVERT_PREFIX, ble::clamp_speaker_name("a very long speaker name indeed")).len() <= ble::ADVERT_NAME_MAX,
            true
        );
    }

    #[test]
    fn bt_messages_round_trip() {
        // Every internally-tagged variant must serialize (newtype-of-Vec
        // would panic at runtime, not compile time).
        let msg = ServerMessage::BtDevices {
            devices: vec![BtDevice {
                address: "AA:BB:CC:DD:EE:FF".into(),
                name: "Phone".into(),
                connected: true,
                kind: "phone".into(),
            }],
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "bt_devices");
        assert_eq!(json["devices"][0]["address"], "AA:BB:CC:DD:EE:FF");
        let back: ServerMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);

        let action: ClientMessage = serde_json::from_str(
            r#"{"type":"bt_device","address":"AA:BB:CC:DD:EE:FF","action":"disconnect"}"#,
        )
        .unwrap();
        assert_eq!(
            action,
            ClientMessage::BtDevice {
                address: "AA:BB:CC:DD:EE:FF".into(),
                action: BtDeviceAction::Disconnect,
            }
        );
    }

    #[test]
    fn client_message_json_shape() {
        let json = serde_json::to_value(ClientMessage::SetVolume { level: 0.5 }).unwrap();
        assert_eq!(json["type"], "set_volume");
        assert_eq!(json["level"], 0.5);

        let unit: ClientMessage = serde_json::from_str(r#"{"type":"play"}"#).unwrap();
        assert_eq!(unit, ClientMessage::Play);

        let pairing: ClientMessage =
            serde_json::from_str(r#"{"type":"pairing","action":"enable"}"#).unwrap();
        assert_eq!(
            pairing,
            ClientMessage::Pairing {
                action: PairingAction::Enable
            }
        );
    }

    #[test]
    fn wifi_messages_round_trip() {
        let msg = ServerMessage::Wifi(WifiState {
            supported: true,
            enabled: true,
            connected: Some("Home".into()),
            ip: Some("192.168.1.7/24".into()),
            ap_active: false,
            ap_ssid: None,
            saved: vec!["Home".into(), "Cabin".into()],
            settings_url: Some("http://192.168.1.7/".into()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "wifi");
        assert_eq!(json["connected"], "Home");
        assert!(json.get("ap_ssid").is_none());
        let back: ServerMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);

        let rejoin: ClientMessage =
            serde_json::from_str(r#"{"type":"wifi","action":"rejoin","ssid":"Home"}"#).unwrap();
        assert_eq!(
            rejoin,
            ClientMessage::Wifi(WifiAction::Rejoin {
                ssid: "Home".into()
            })
        );
        let ap =
            serde_json::to_value(ClientMessage::Wifi(WifiAction::Ap { enabled: true })).unwrap();
        assert_eq!(ap["type"], "wifi");
        assert_eq!(ap["action"], "ap");
        assert_eq!(ap["enabled"], true);
        let disc: ClientMessage =
            serde_json::from_str(r#"{"type":"wifi","action":"disconnect"}"#).unwrap();
        assert_eq!(disc, ClientMessage::Wifi(WifiAction::Disconnect));
    }

    #[test]
    fn ble_chunking_round_trip() {
        // Multi-chunk message survives reassembly.
        let msg: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let chunks = ble::chunk_message(&msg, ble::DEFAULT_CHUNK);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= ble::DEFAULT_CHUNK));
        assert_eq!(chunks[0][0] & ble::CHUNK_FIRST, ble::CHUNK_FIRST);
        assert_eq!(chunks.last().unwrap()[0] & ble::CHUNK_LAST, ble::CHUNK_LAST);
        let mut r = ble::Reassembler::default();
        let mut out = None;
        for (i, c) in chunks.iter().enumerate() {
            let res = r.push(c);
            if i + 1 < chunks.len() {
                assert!(res.is_none());
            } else {
                out = res;
            }
        }
        assert_eq!(out.unwrap(), msg);

        // Single-chunk (and empty) messages carry FIRST|LAST.
        let one = ble::chunk_message(b"hi", 100);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0][0], ble::CHUNK_FIRST | ble::CHUNK_LAST);
        assert_eq!(ble::Reassembler::default().push(&one[0]).unwrap(), b"hi");
        let empty = ble::chunk_message(b"", 100);
        assert_eq!(empty, vec![vec![ble::CHUNK_FIRST | ble::CHUNK_LAST]]);

        // Degenerate max_chunk is clamped to 2, never exceeded.
        for max in [0, 1, 2] {
            let tiny = ble::chunk_message(b"abc", max);
            assert!(tiny.iter().all(|c| c.len() <= 2), "max_chunk={max}");
            let mut r = ble::Reassembler::default();
            let mut out = None;
            for c in &tiny {
                out = r.push(c);
            }
            assert_eq!(out.unwrap(), b"abc", "max_chunk={max}");
        }

        // Continuation without a start is dropped; resync on next FIRST.
        let mut r = ble::Reassembler::default();
        assert!(r.push(&[0x00, 1, 2, 3]).is_none());
        assert!(r.push(&[ble::CHUNK_LAST, 4]).is_none());
        assert_eq!(
            r.push(&[ble::CHUNK_FIRST | ble::CHUNK_LAST, 9]).unwrap(),
            vec![9]
        );
    }

    #[test]
    fn visualizer_frame_round_trip() {
        let bars = vec![0u16, 1, 512, u16::MAX];
        let frame = encode_visualizer_frame(&bars);
        assert_eq!(frame[0], frame_tag::VISUALIZER);
        assert_eq!(frame.len(), 1 + bars.len() * 2);
        assert_eq!(decode_visualizer_frame(&frame).unwrap(), bars);

        assert!(decode_visualizer_frame(&[]).is_none());
        assert!(decode_visualizer_frame(&[0xFF, 0x01, 0x02]).is_none());
        assert!(decode_visualizer_frame(&[frame_tag::VISUALIZER, 0x01]).is_none());
    }

    #[test]
    fn artwork_frame_round_trip() {
        let jpeg = [0xFFu8, 0xD8, 0xFF, 0xE0, 0x42];
        let frame = encode_artwork_frame(&jpeg);
        assert_eq!(frame[0], frame_tag::ARTWORK);
        assert_eq!(decode_artwork_frame(&frame).unwrap(), &jpeg);
        assert!(decode_artwork_frame(&[frame_tag::VISUALIZER, 1, 2]).is_none());
        assert!(decode_artwork_frame(&[]).is_none());
    }
}
