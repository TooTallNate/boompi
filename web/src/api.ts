import type { Settings, SettingsPatch, StateResponse } from "./proto";

export async function fetchState(): Promise<StateResponse> {
  const r = await fetch("/api/state");
  if (!r.ok) throw new Error(`state fetch failed: HTTP ${r.status}`);
  return r.json();
}

/** POST any protocol ClientMessage over HTTP. */
export async function sendCommand(msg: object): Promise<void> {
  const r = await fetch("/api/command", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(msg),
  });
  if (!r.ok) throw new Error(`command failed: HTTP ${r.status}`);
}

export interface WifiNetwork {
  ssid: string;
  signal: number;
  security: string;
  in_use: boolean;
  saved: boolean;
}

export interface WifiStatus {
  supported: boolean;
  enabled: boolean;
  connected: string | null;
  ip: string | null;
  ap_active: boolean;
  networks: WifiNetwork[];
  saved: string[];
}

export type WifiAction =
  | { action: "connect"; ssid: string; psk?: string }
  | { action: "forget"; name: string }
  | { action: "radio"; enabled: boolean }
  | { action: "ap"; enabled: boolean };

export async function fetchWifi(): Promise<WifiStatus> {
  const r = await fetch("/api/wifi");
  const body = await r.json();
  if (!r.ok) throw new Error(body.error ?? `HTTP ${r.status}`);
  return body;
}

export async function wifiAction(action: WifiAction): Promise<WifiStatus> {
  const r = await fetch("/api/wifi", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(action),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error ?? `HTTP ${r.status}`);
  return body;
}

export interface ClockStatus {
  timezone: string;
  ntp: boolean;
  synchronized: boolean;
  now_ms: number;
  timezones: string[];
}

export async function fetchClock(): Promise<ClockStatus> {
  const r = await fetch("/api/clock");
  const body = await r.json();
  if (!r.ok) throw new Error(body.error ?? `HTTP ${r.status}`);
  return body;
}

export async function patchClock(patch: {
  timezone?: string;
  ntp?: boolean;
}): Promise<ClockStatus> {
  const r = await fetch("/api/clock", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(patch),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.error ?? `HTTP ${r.status}`);
  return body;
}

export async function patchSettings(patch: SettingsPatch): Promise<Settings> {
  const r = await fetch("/api/settings", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(patch),
  });
  if (!r.ok) throw new Error(`settings update failed: HTTP ${r.status}`);
  return r.json();
}
