// Hand-mirrored subset of boompi-proto (rust/boompi-proto/src/lib.rs).
// Keep field names in sync - serde uses snake_case throughout.

export type Theme = "dark" | "light";

export type UpdateChannel = "stable" | "edge";

export type ScreensaverKind = "off" | "clock" | "matrix" | "art";

export interface Settings {
  ui_scale: number;
  /** Advertised AirPlay model - senders pick their picker icon from it.
      "" = generic speaker. */
  airplay_model: string;
  name: string;
  theme: Theme;
  online_art_fallback: boolean;
  update_channel: UpdateChannel;
  airplay_classic: boolean;
  screensaver: ScreensaverKind;
  screensaver_min: number;
}

export interface SettingsPatch {
  ui_scale?: number;
  airplay_model?: string;
  name?: string;
  theme?: Theme;
  online_art_fallback?: boolean;
  update_channel?: UpdateChannel;
  airplay_classic?: boolean;
  screensaver?: ScreensaverKind;
  screensaver_min?: number;
}

export type PairingState = "idle" | "discoverable" | "confirm" | "unavailable";

export interface Pairing {
  state: PairingState;
  device_name?: string;
  passkey?: number;
}

export type PairingAction = "enable" | "cancel" | "confirm" | "reject";

export type BtVolumeMode = "auto" | "phone" | "speaker";

export interface BtDevice {
  address: string;
  name: string;
  connected: boolean;
  volume_mode: BtVolumeMode;
  volume_mode_auto: BtVolumeMode;
}

export interface EmojiFontInfo {
  id: string;
  label: string;
  license: string;
  installed: boolean;
  active: boolean;
  builtin: boolean;
  size: number;
}

export interface EmojiFontsState {
  fonts: EmojiFontInfo[];
  downloading: string | null;
  progress: number | null;
  error: string | null;
}

export type EmojiFontAction = "download" | "select" | "remove";

export type UpdateStage =
  | "downloading_system"
  | "verifying_system"
  | "downloading_boot"
  | "verifying_boot"
  | "restarting";

export interface UpdateState {
  /** Running OS image version: "v2.0.0", "v2.0.0-abcdefg" or "dev". */
  version: string;
  available: string | null;
  checking: boolean;
  applying: string | null;
  stage: UpdateStage | null;
  progress: number | null;
  error: string | null;
}

export type UpdateAction = "check" | "apply";

export type BtDeviceAction =
  | "connect"
  | "disconnect"
  | "remove"
  | { set_volume_mode: { mode: BtVolumeMode } };

export interface Hello {
  proto_version: number;
  name: string;
  model?: string;
  version: string;
  uptime_secs: number;
}

export interface AppState {
  settings: Settings;
  volume: number;
  pairing: Pairing;
  bt_devices: BtDevice[];
  setup: { required: boolean; wifi_status?: unknown };
  emoji_fonts: EmojiFontsState;
  updates: UpdateState;
  // Present but unused by the settings UI so far:
  source: unknown;
  track: unknown;
  battery: unknown;
}

export interface StateResponse {
  hello: Hello;
  state: AppState;
}

/** Server → client WebSocket messages (subset the settings UI reacts to). */
export type ServerMessage =
  | ({ type: "hello" } & Hello)
  | { type: "state"; [k: string]: unknown }
  | ({ type: "settings" } & Settings)
  | ({ type: "emoji_fonts" } & EmojiFontsState)
  | ({ type: "update" } & UpdateState)
  | ({ type: "pairing" } & Pairing)
  | { type: "bt_devices"; devices: BtDevice[] }
  | { type: "volume"; level: number }
  | { type: string; [k: string]: unknown };

/** Client → server WebSocket messages used by the settings UI. */
export type ClientMessage =
  | { type: "emoji_font"; action: EmojiFontAction; id: string }
  | { type: "update"; action: UpdateAction }
  | { type: "pairing"; action: PairingAction }
  | { type: "bt_device"; address: string; action: BtDeviceAction }
  | { type: "factory_reset" };
