// Hand-mirrored subset of boompi-proto (rust/boompi-proto/src/lib.rs).
// Keep field names in sync — serde uses snake_case throughout.

export type Theme = "dark" | "light";

export interface Settings {
  /** Advertised AirPlay model — senders pick their picker icon from it.
      "" = generic speaker. */
  airplay_model: string;
  name: string;
  theme: Theme;
  online_art_fallback: boolean;
}

export interface SettingsPatch {
  airplay_model?: string;
  name?: string;
  theme?: Theme;
  online_art_fallback?: boolean;
}

export type PairingState = "idle" | "discoverable" | "confirm" | "unavailable";

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

export type BtDeviceAction = "connect" | "disconnect" | "remove";

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
  setup: { required: boolean };
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
  | ({ type: "pairing" } & Pairing)
  | { type: "bt_devices"; devices: BtDevice[] }
  | { type: "volume"; level: number }
  | { type: string; [k: string]: unknown };

/** Client → server WebSocket messages used by the settings UI. */
export type ClientMessage =
  | { type: "pairing"; action: PairingAction }
  | { type: "bt_device"; address: string; action: BtDeviceAction }
  | { type: "factory_reset" };
