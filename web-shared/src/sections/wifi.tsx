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
import {
  useBoompi,
  type WifiNetwork,
  type WifiRestAction,
  type WifiStatus,
} from "@boompi/ui/transport";
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

/** Requires an IP path (REST scan results); hidden on BLE-only links
 *  except for the hotspot toggle, which rides the protocol. */
export function WifiSection() {
  const { rest, send, state } = useBoompi();
  const [wifi, setWifi] = useState<WifiStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [joining, setJoining] = useState<string | null>(null); // ssid with open psk prompt
  const [psk, setPsk] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!rest) return;
    try {
      setWifi(await rest.fetchWifi());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [rest]);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 15000);
    return () => clearInterval(t);
  }, [refresh]);

  // BLE-only: offer what the protocol carries - live link state and the
  // hotspot toggle (the escape hatch that creates an IP path).
  if (!rest) {
    const w = state?.wifi;
    return (
      <Card>
        <CardHeader>
          <CardTitle>Wi-Fi</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {w?.connected ? (
            <p className="text-sm">
              Connected to <strong>{w.connected}</strong>
              {w.ip ? ` (${w.ip})` : ""}
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">
              Not connected to Wi-Fi.
            </p>
          )}
          <Field orientation="horizontal">
            <div className="flex flex-col gap-1">
              <FieldLabel htmlFor="wifi-hotspot">Hotspot</FieldLabel>
              <FieldDescription>
                Speaker broadcasts its own network - join it to reach the full
                settings page ({w?.settings_url ?? "shown on the speaker"}).
              </FieldDescription>
            </div>
            <Switch
              id="wifi-hotspot"
              checked={w?.ap_active ?? false}
              onCheckedChange={(v) => send({ type: "wifi", action: "ap", enabled: v })}
            />
          </Field>
          <Alert>
            <AlertDescription>
              Network scanning and joining need an IP connection - use the
              speaker's panel or open the settings page over Wi-Fi.
            </AlertDescription>
          </Alert>
        </CardContent>
      </Card>
    );
  }

  async function act(a: WifiRestAction) {
    if (!rest) return;
    setBusy(true);
    setError(null);
    try {
      setWifi(await rest.wifiAction(a));
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
        {!wifi ? (
          <p className="text-sm text-muted-foreground">{error ?? "loading…"}</p>
        ) : !wifi.supported ? (
          <p className="text-sm text-muted-foreground">No Wi-Fi hardware.</p>
        ) : (
          <>
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2">
                Wi-Fi
                {wifi.connected && (
                  <span className="text-xs text-success">
                    {wifi.connected}
                    {wifi.ip ? ` (${wifi.ip})` : ""}
                  </span>
                )}
                {wifi.ap_active && <Badge>hotspot active</Badge>}
              </span>
              {/* No radio toggle while the setup hotspot is up: turning the
                  radio off would kill this very connection (and the captive
                  portal with it). */}
              {!wifi.ap_active && (
                <Switch
                  checked={wifi.enabled}
                  onCheckedChange={(v) => act({ action: "radio", enabled: v })}
                />
              )}
            </div>

            {wifi.ap_active && (
              <Alert>
                <AlertDescription>
                  You're connected through the speaker's own hotspot. The
                  networks below were found just before the hotspot started.
                  Joining one switches the hotspot off while the speaker
                  connects - rejoin your normal Wi-Fi afterwards. If the
                  password is wrong, the hotspot comes back within a minute so
                  you can retry.
                </AlertDescription>
              </Alert>
            )}

            {/* Hotspot: the speaker broadcasts its own open network so a
                phone can reach this page with no shared Wi-Fi at all -
                camping mode. Hidden while it is the connection being used. */}
            {!wifi.ap_active && (
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
                    checked={wifi.ap_active}
                    onCheckedChange={(v) => act({ action: "ap", enabled: v })}
                  />
                </Field>
              </>
            )}

            {wifi.enabled &&
              wifi.networks.map((n) => (
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
