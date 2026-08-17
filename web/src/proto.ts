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
  clock_24h: boolean;
  game_volume: number;
  visualizer_opacity: number;
  mqtt_broker: string;
  mqtt_username: string;
  mqtt_password: string;
  screensaver: ScreensaverKind;
  screensaver_min: number;
}

export interface SettingsPatch {
  ui_scale?: number;
  visualizer_opacity?: number;
  airplay_model?: string;
  name?: string;
  theme?: Theme;
  online_art_fallback?: boolean;
  update_channel?: UpdateChannel;
  airplay_classic?: boolean;
  clock_24h?: boolean;
  game_volume?: number;
  mqtt_broker?: string;
  mqtt_username?: string;
  mqtt_password?: string;
  screensaver?: ScreensaverKind;
  screensaver_min?: number;
}

export type PairingState =
  | "idle"
  | "discoverable"
  | "confirm"
  | "pairing" // gamepad autopair in progress - informational, no decision
  | "unavailable";

export interface Pairing {
  state: PairingState;
  device_name?: string;
  passkey?: number;
}

export type PairingAction = "enable" | "cancel" | "confirm" | "reject";


export interface BtDevice {
  address: string;
  name: string;
  connected: boolean;
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
  | "remove";

/** Wi-Fi link + hotspot state mirrored to all clients (scan results
    stay on GET /api/wifi). */
export interface WifiState {
  supported: boolean;
  enabled: boolean;
  connected?: string;
  ip?: string;
  ap_active: boolean;
  ap_ssid?: string;
  saved: string[];
  /** Settings-UI URL reachable right now (hotspot gateway while
      ap_active). */
  settings_url?: string;
}

export type WifiAction =
  | { action: "rejoin"; ssid: string }
  | { action: "disconnect" }
  | { action: "forget"; ssid: string }
  | { action: "ap"; enabled: boolean };

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
  wifi: WifiState;
  emoji_fonts: EmojiFontsState;
  updates: UpdateState;
  // Present but unused by the settings UI so far:
  source: unknown;
  track: unknown;
  battery: Battery | null;
  games: GamesState;
  battery_status?: "unconfigured" | "error" | "ok";
  battery_status_detail?: string;
}

export interface Game {
  system: string;
  file: string;
  name: string;
  size: number;
}

export interface GamesState {
  games: Game[];
  running?: string;
  gamepad: boolean;
  storage_free: number;
  storage_total: number;
}

/** INA260 battery telemetry; null on boxes without one. */
export interface Battery {
  voltage: number;
  current: number;
  power: number;
  /** 0.0-1.0 state of charge. */
  percentage: number;
  charging: boolean;
  full: boolean;
  low: boolean;
  time_remaining_secs?: number;
  ts: number;
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
  | ({ type: "wifi" } & WifiState)
  | { type: "bt_devices"; devices: BtDevice[] }
  | { type: "volume"; level: number }
  | ({ type: "battery" } & Battery)
  | ({ type: "games" } & GamesState)
  | { type: string; [k: string]: unknown };

/** Client → server WebSocket messages used by the settings UI. */
export type ClientMessage =
  | { type: "set_volume"; level: number }
  | { type: "emoji_font"; action: EmojiFontAction; id: string }
  | { type: "update"; action: UpdateAction }
  | { type: "preview_screensaver" }
  | { type: "reboot" }
  | { type: "game"; action: "launch"; system: string; file: string }
  | { type: "game"; action: "stop" }
  | { type: "pairing"; action: PairingAction }
  | { type: "bt_device"; address: string; action: BtDeviceAction }
  | ({ type: "wifi" } & WifiAction)
  /** Offer this device's clock as a fallback time source (no-op when
   * the box is NTP-synchronized). */
  | { type: "set_time"; epoch_ms: number };
