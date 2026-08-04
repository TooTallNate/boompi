// Hand-mirrored subset of boompi-proto (rust/boompi-proto/src/lib.rs).
// Keep field names in sync — serde uses snake_case throughout.

export type Theme = "dark" | "light";

export interface Settings {
  name: string;
  theme: Theme;
  online_art_fallback: boolean;
}

export interface SettingsPatch {
  name?: string;
  theme?: Theme;
  online_art_fallback?: boolean;
}

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
  // Present but unused by the settings UI so far:
  source: unknown;
  track: unknown;
  battery: unknown;
  pairing: unknown;
  setup: { required: boolean };
}

export interface StateResponse {
  hello: Hello;
  state: AppState;
}
