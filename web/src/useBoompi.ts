import { useCallback, useEffect, useRef, useState } from "react";
import { fetchState } from "./api";
import type {
  AppState,
  ClientMessage,
  Hello,
  ServerMessage,
  Settings,
} from "./proto";

/**
 * Live connection to boompid: initial REST snapshot + WebSocket updates
 * (settings, pairing state, device list). Reconnects with a fixed backoff.
 */
export function useBoompi() {
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
      ws.onmessage = (ev) => {
        if (typeof ev.data !== "string") return; // binary frames: art/visualizer
        const msg = JSON.parse(ev.data) as ServerMessage;
        switch (msg.type) {
          case "hello":
            setHello(msg as Hello & { type: "hello" });
            break;
          case "state":
            setState(msg as unknown as AppState);
            setError(null);
            break;
          case "settings":
            setState((s) => s && { ...s, settings: msg as unknown as Settings });
            break;
          case "pairing":
            setState((s) => s && { ...s, pairing: msg as never });
            break;
          case "bt_devices":
            setState(
              (s) => s && { ...s, bt_devices: (msg as never)["devices"] },
            );
            break;
          case "volume":
            setState((s) => s && { ...s, volume: (msg as never)["level"] });
            break;
          case "emoji_fonts": {
            const { type: _t, ...emoji } = msg as never as Record<string, unknown>;
            setState((s) => s && { ...s, emoji_fonts: emoji as never });
            break;
          }
          case "setup":
            setState(
              (s) =>
                s && { ...s, setup: { required: (msg as never)["required"] } },
            );
            break;
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

  const applySettings = useCallback((settings: Settings) => {
    setState((s) => s && { ...s, settings });
  }, []);

  return { hello, state, error, send, applySettings };
}
