// Transport abstraction: the sections work against this interface so
// the same UI runs on the box's web app (WebSocket + REST) and the
// hosted remote app (Web Bluetooth GATT, no IP path at all).

import { createContext, useContext, useState } from "react";
import type {
  AppState,
  ClientMessage,
  Hello,
  Settings,
  SettingsPatch,
} from "./proto";

// --- REST-only shapes (unavailable over BLE) -------------------------------

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

export type WifiRestAction =
  | { action: "connect"; ssid: string; psk?: string }
  | { action: "forget"; name: string }
  | { action: "disconnect" }
  | { action: "radio"; enabled: boolean }
  | { action: "ap"; enabled: boolean };

export interface ClockStatus {
  timezone: string;
  ntp: boolean;
  synchronized: boolean;
  now_ms: number;
  timezones: string[];
}

/** APIs that require an IP path to the speaker (REST). `rest: null`
 *  on BLE-only links hides the sections that depend on them. */
export interface RestApis {
  fetchWifi(): Promise<WifiStatus>;
  wifiAction(a: WifiRestAction): Promise<WifiStatus>;
  fetchClock(): Promise<ClockStatus>;
  patchClock(p: { timezone?: string; ntp?: boolean }): Promise<ClockStatus>;
  uploadGames(system: string, files: FileList): Promise<void>;
  deleteGame(system: string, file: string): Promise<void>;
  /** Hostname for user-facing hints (smb:// share). */
  host: string;
}

// --- The connection --------------------------------------------------------

export interface BoompiConnection {
  hello: Hello | null;
  state: AppState | null;
  /** Connection-level error to surface ("connection lost - retrying"). */
  error: string | null;
  send(msg: ClientMessage): void;
  /** Persist a settings patch. REST transports return the server's
   *  updated Settings; BLE transports apply optimistically and rely on
   *  the Settings broadcast to correct drift. */
  saveSettings(patch: SettingsPatch): Promise<Settings>;
  rest: RestApis | null;
}

export const BoompiContext = createContext<BoompiConnection | null>(null);

export function useBoompi(): BoompiConnection {
  const conn = useContext(BoompiContext);
  if (!conn) throw new Error("BoompiContext missing");
  return conn;
}

// --- Save-status helper -----------------------------------------------------

export type SaveStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "ok" }
  | { kind: "err"; message: string };

/** Settings-patch submit with inline status ("saving… / saved / err"). */
export function useSave() {
  const conn = useBoompi();
  const [status, setStatus] = useState<SaveStatus>({ kind: "idle" });

  async function save(patch: SettingsPatch) {
    setStatus({ kind: "saving" });
    try {
      await conn.saveSettings(patch);
      setStatus({ kind: "ok" });
      setTimeout(() => setStatus({ kind: "idle" }), 2500);
    } catch (e) {
      setStatus({ kind: "err", message: String(e) });
    }
  }

  return { status, save };
}

export function formatDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}
