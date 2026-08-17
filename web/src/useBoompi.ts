import { useCallback, useEffect, useRef, useState } from "react";
import type { AppState, ClientMessage, Hello, ServerMessage, SettingsPatch } from "@boompi/ui/proto";
import { applyServerMessage, type BoompiConnection } from "@boompi/ui/transport";
import { fetchState, patchSettings, restApis } from "./api";

/** The box-app transport: WebSocket for live state, REST for the
 *  IP-only extras. Implements the shared BoompiConnection interface. */
export function useBoompi(): BoompiConnection {
  const [hello, setHello] = useState<Hello | null>(null);
  const [state, setState] = useState<AppState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    let closed = false;
    let ws: WebSocket | null = null;
    let retry: ReturnType<typeof setTimeout> | null = null;

    function connect() {
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(`${proto}//${location.host}/ws`);
      wsRef.current = ws;
      ws.onopen = () => {
        // The box has no RTC; offer our clock as a fallback time
        // source. Ignored server-side whenever NTP is synchronized.
        ws?.send(JSON.stringify({ type: "set_time", epoch_ms: Date.now() }));
      };
      ws.onmessage = (ev) => {
        if (typeof ev.data !== "string") return; // binary frames: art/visualizer
        const msg = JSON.parse(ev.data) as ServerMessage;
        if (msg.type === "hello") {
          setHello(msg as Hello & { type: "hello" });
        } else if (msg.type === "state") {
          setState(msg as unknown as AppState);
          setError(null);
        } else {
          setState((s) => s && applyServerMessage(s, msg as never));
        }
      };
      ws.onclose = () => {
        wsRef.current = null;
        if (!closed) {
          setError("connection lost - retrying…");
          retry = setTimeout(connect, 2000);
        }
      };
    }

    // REST snapshot first (also serves as a fast-fail error message),
    // then the socket keeps everything fresh.
    fetchState()
      .then((data) => {
        setHello(data.hello);
        setState(data.state);
        connect();
      })
      .catch((e) => setError(String(e)));

    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      ws?.close();
    };
  }, []);

  const send = useCallback((msg: ClientMessage) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(msg));
    }
  }, []);

  const saveSettings = useCallback(async (patch: SettingsPatch) => {
    const settings = await patchSettings(patch);
    setState((s) => s && { ...s, settings });
    return settings;
  }, []);

  return { hello, state, error, send, saveSettings, rest: restApis };
}
