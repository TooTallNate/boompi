import { useCallback, useEffect, useState } from "react";
import { Alert, AlertDescription } from "@boompi/ui/components/alert";
import { Badge } from "@boompi/ui/components/badge";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { Separator } from "@boompi/ui/components/separator";
import { Switch } from "@boompi/ui/components/switch";
import { cn } from "@boompi/ui/lib/utils";
import type { WifiNetwork } from "@boompi/ui/proto";
import { useBoompi, type WifiRestAction } from "@boompi/ui/transport";
import { Lock } from "lucide-react";

function SignalBars({ signal }: { signal: number }) {
  const bars = signal > 75 ? 4 : signal > 50 ? 3 : signal > 25 ? 2 : 1;
  return (
    <span className="flex items-end gap-px" title={`${signal}%`}>
      {[1, 2, 3, 4].map((b) => (
        <span
          key={b}
          className={cn("w-[3px] rounded-sm", b <= bars ? "bg-foreground" : "bg-border")}
          style={{ height: 3 + b * 3 }}
        />
      ))}
    </span>
  );
}

/** A transport-independent view of Wi-Fi. REST fills it from
 *  GET/POST /api/wifi (synchronous errors); protocol links (BLE) fill
 *  it from WifiState broadcasts + wifi_networks scan broadcasts. */
interface WifiView {
  supported: boolean;
  enabled: boolean;
  connected: string | null;
  ip: string | null;
  ap_active: boolean;
  networks: WifiNetwork[];
}

/** Fully functional on every transport: scans and password-joins ride
 *  the protocol (WifiAction::Scan / Connect), so the BLE-only remote
 *  manages Wi-Fi the same as the box's own web app. */
export function WifiSection() {
  const { rest, send, state } = useBoompi();
  const [restView, setRestView] = useState<WifiView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [joining, setJoining] = useState<string | null>(null); // ssid with open psk prompt
  const [psk, setPsk] = useState("");
  const [busy, setBusy] = useState(false);

  // REST: poll the status+scan endpoint (returns fresh scan results).
  // Protocol: request a scan; results arrive as wifi_networks
  // broadcasts and land in state.
  const refresh = useCallback(async () => {
    if (rest) {
      try {
        setRestView(await rest.fetchWifi());
        setError(null);
      } catch (e) {
        setError(String(e));
      }
    } else {
      send({ type: "wifi", action: "scan" });
    }
  }, [rest, send]);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 15000);
    return () => clearInterval(t);
  }, [refresh]);

  const view: WifiView | null = rest
    ? restView
    : state?.wifi
      ? {
          supported: state.wifi.supported,
          enabled: state.wifi.enabled,
          connected: state.wifi.connected ?? null,
          ip: state.wifi.ip ?? null,
          ap_active: state.wifi.ap_active,
          networks: state.wifi_networks ?? [],
        }
      : null;

  async function act(a: WifiRestAction) {
    setBusy(true);
    setError(null);
    try {
      if (rest) {
        setRestView(await rest.wifiAction(a));
      } else {
        // Same actions over the protocol; results come back as state
        // broadcasts. (`forget` differs in field name only.)
        send({
          type: "wifi",
          ...(a.action === "forget" ? { action: "forget", ssid: a.name } : a),
        } as never);
        // A join kicked off over BLE reports through WifiJoinStatus /
        // Wifi broadcasts; refresh the list shortly after.
        setTimeout(() => send({ type: "wifi", action: "scan" }), 3000);
      }
      setJoining(null);
      setPsk("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function join(net: WifiNetwork) {
    if (net.saved || net.security === "") {
      act({ action: "connect", ssid: net.ssid });
    } else {
      setJoining(net.ssid);
      setPsk("");
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Wi-Fi</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {!view ? (
          <p className="text-sm text-muted-foreground">{error ?? "loading…"}</p>
        ) : !view.supported ? (
          <p className="text-sm text-muted-foreground">No Wi-Fi hardware.</p>
        ) : (
          <>
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2">
                Wi-Fi
                {view.connected && (
                  <span className="text-xs text-success">
                    {view.connected}
                    {view.ip ? ` (${view.ip})` : ""}
                  </span>
                )}
                {view.ap_active && <Badge>hotspot active</Badge>}
              </span>
              {/* No radio toggle while the setup hotspot is up: turning the
                  radio off would kill this very connection (and the captive
                  portal with it). */}
              {!view.ap_active && (
                <Switch
                  checked={view.enabled}
                  onCheckedChange={(v) => act({ action: "radio", enabled: v })}
                />
              )}
            </div>

            {view.ap_active && (
              <Alert>
                <AlertDescription>
                  {rest ? (
                    <>
                      You're connected through the speaker's own hotspot. The
                      networks below were found just before the hotspot
                      started. Joining one switches the hotspot off while the
                      speaker connects - rejoin your normal Wi-Fi afterwards.
                      If the password is wrong, the hotspot comes back within
                      a minute so you can retry.
                    </>
                  ) : (
                    <>
                      The speaker's hotspot is broadcasting. Joining a network
                      below switches the hotspot off - your Bluetooth
                      connection here survives either way.
                    </>
                  )}
                </AlertDescription>
              </Alert>
            )}

            {/* Hotspot: the speaker broadcasts its own open network so a
                phone can reach the web UI with no shared Wi-Fi at all -
                camping mode. Hidden while it is the connection being used. */}
            {!view.ap_active && (
              <>
                <Separator />
                <Field orientation="horizontal">
                  <div className="flex flex-col gap-1">
                    <FieldLabel htmlFor="hotspot">Hotspot</FieldLabel>
                    <FieldDescription>
                      Speaker broadcasts its own network - control it anywhere,
                      no shared Wi-Fi needed. Turning it on drops the speaker
                      off this network.
                    </FieldDescription>
                  </div>
                  <Switch
                    id="hotspot"
                    checked={view.ap_active}
                    onCheckedChange={(v) => act({ action: "ap", enabled: v })}
                  />
                </Field>
              </>
            )}

            {view.enabled && view.networks.length === 0 && !rest && (
              <p className="text-xs text-muted-foreground">scanning…</p>
            )}
            {view.enabled &&
              view.networks.map((n) => (
                <div key={n.ssid} className="flex flex-col gap-2 border-t pt-3">
                  <div className="flex items-center justify-between gap-3">
                    <button
                      className="flex min-w-0 flex-1 items-center gap-2 text-left"
                      onClick={() => join(n)}
                      disabled={busy || n.in_use}
                    >
                      <SignalBars signal={n.signal} />
                      <span className="truncate">{n.ssid}</span>
                      {n.security !== "" && (
                        <Lock className="size-3 text-muted-foreground" aria-label="secured" />
                      )}
                      {n.in_use && <Badge variant="secondary">connected</Badge>}
                      {n.saved && !n.in_use && (
                        <span className="text-xs text-muted-foreground">saved</span>
                      )}
                    </button>
                    {n.in_use && (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => act({ action: "disconnect" })}
                        disabled={busy}
                        title="Leave this network without forgetting its password"
                      >
                        Disconnect
                      </Button>
                    )}
                    {(n.saved || n.in_use) && (
                      <Button
                        variant="destructive"
                        size="sm"
                        onClick={() => act({ action: "forget", name: n.ssid })}
                        disabled={busy}
                      >
                        Forget
                      </Button>
                    )}
                  </div>
                  {joining === n.ssid && (
                    <form
                      className="flex gap-2"
                      onSubmit={(e) => {
                        e.preventDefault();
                        act({ action: "connect", ssid: n.ssid, psk });
                      }}
                    >
                      <Input
                        type="password"
                        autoFocus
                        placeholder="Password"
                        className="min-w-0 flex-1"
                        value={psk}
                        onChange={(e) => setPsk(e.target.value)}
                      />
                      <Button type="submit" disabled={psk.length < 8 || busy}>
                        Join
                      </Button>
                      <Button type="button" variant="outline" onClick={() => setJoining(null)}>
                        Cancel
                      </Button>
                    </form>
                  )}
                </div>
              ))}
            {error && <p className="text-xs text-destructive">{error}</p>}
          </>
        )}
      </CardContent>
    </Card>
  );
}
