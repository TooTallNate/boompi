import { useCallback, useRef, useState } from "react";
import type { AppState, ClientMessage, Hello, Settings, SettingsPatch } from "@boompi/ui/proto";
import { applyServerMessage, type BoompiConnection } from "@boompi/ui/transport";
import { BleLink } from "./ble";

export type BleStatus = "idle" | "connecting" | "connected" | "lost";

/** BLE-backed BoompiConnection: same protocol as the WebSocket, no IP
 *  path, `rest: null` (sections degrade gracefully). */
export function useBoompiBle(): {
  conn: BoompiConnection;
  status: BleStatus;
  connect(): Promise<void>;
  disconnect(): void;
} {
  const [status, setStatus] = useState<BleStatus>("idle");
  const [hello, setHello] = useState<Hello | null>(null);
  const [state, setState] = useState<AppState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const link = useRef<BleLink | null>(null);

  const connect = useCallback(async () => {
    setStatus("connecting");
    setError(null);
    try {
      link.current = await BleLink.connect({
        onMessage: (msg) => {
          if (msg.type === "hello") {
            setHello(msg as never);
          } else if (msg.type === "state") {
            setState(msg as never);
            setStatus("connected");
          } else {
            setState((s) => s && applyServerMessage(s, msg as never));
          }
        },
        onDisconnect: () => {
          setStatus("lost");
          setError("Bluetooth link lost");
        },
      });
      // The box has no RTC; offer this device's clock (ignored while
      // the box is NTP-synchronized). Same greeting the web app sends.
      link.current.send({ type: "set_time", epoch_ms: Date.now() } as never);
    } catch (e) {
      setStatus("idle");
      // User cancelling the chooser is not an error worth surfacing.
      if ((e as Error).name !== "NotFoundError") setError(String(e));
      throw e;
    }
  }, []);

  const disconnect = useCallback(() => {
    link.current?.disconnect();
    link.current = null;
    setStatus("idle");
    setHello(null);
    setState(null);
    setError(null);
  }, []);

  const send = useCallback((msg: ClientMessage) => {
    link.current?.send(msg);
  }, []);

  const saveSettings = useCallback(async (patch: SettingsPatch) => {
    // No request/response over BLE: apply optimistically and send; the
    // server's Settings broadcast corrects any drift.
    let next: Settings | null = null;
    setState((s) => {
      if (!s) return s;
      next = { ...s.settings, ...patch };
      return { ...s, settings: next };
    });
    send({ type: "set_settings", ...patch } as never);
    if (!next) throw new Error("not connected");
    return next;
  }, [send]);

  return {
    conn: { hello, state, error, send, saveSettings, rest: null },
    status,
    connect,
    disconnect,
  };
}
