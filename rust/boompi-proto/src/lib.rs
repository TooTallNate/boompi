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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub active: Option<SourceKind>,
    pub device_name: Option<String>,
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
    /// 0.0-1.0, linear between the configured min/max pack voltages.
    pub percentage: f32,
    pub charging: bool,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BtDevice {
    /// Colon-form address ("6C:3A:FF:58:84:4C") - the id for device actions.
    pub address: String,
    pub name: String,
    pub connected: bool,
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

/// User-adjustable settings, mirrored to all clients.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
}

/// First-boot setup state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetupState {
    /// True until initial configuration has been completed. While set, the
    /// panel shows the onboarding screen and (when Wi-Fi hardware exists
    /// and nothing is connected) the speaker broadcasts its own hotspot.
    pub required: bool,
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
    /// Hardware model hint, e.g. "pi3" / "pi4".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// boompid version.
    pub version: String,
    pub uptime_secs: u64,
    /// Browser URL of the settings web UI (LAN address), when known.
    /// The panel renders this as a QR code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_url: Option<String>,
}

/// Full state snapshot; sent after [`Hello`] and available on demand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub source: SourceInfo,
    pub track: Option<Track>,
    pub volume: f32,
    pub battery: Option<Battery>,
    pub pairing: Pairing,
    /// Paired Bluetooth devices.
    #[serde(default)]
    pub bt_devices: Vec<BtDevice>,
    pub settings: Settings,
    pub setup: SetupState,
}

/// Server → client messages (JSON text frames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello(Hello),
    State(State),
    Source(SourceInfo),
    Track(Track),
    Volume { level: f32 },
    Battery(Battery),
    Pairing(Pairing),
    // NB: struct form - internally-tagged serde can't represent a newtype
    // variant wrapping a sequence.
    BtDevices { devices: Vec<BtDevice> },
    Settings(Settings),
    Setup(SetupState),
}

/// Client → server messages (JSON text frames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Play,
    Pause,
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
    /// Wipe all persistent state (config, Wi-Fi, Bluetooth pairings,
    /// caches) and reboot into first-boot setup. The OS slots are
    /// untouched - this is a data reset, not a reflash.
    FactoryReset,
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
    fn bt_messages_round_trip() {
        // Every internally-tagged variant must serialize (newtype-of-Vec
        // would panic at runtime, not compile time).
        let msg = ServerMessage::BtDevices {
            devices: vec![BtDevice {
                address: "AA:BB:CC:DD:EE:FF".into(),
                name: "Phone".into(),
                connected: true,
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
