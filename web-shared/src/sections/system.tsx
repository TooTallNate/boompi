import { Badge } from "@boompi/ui/components/badge";
import { ConfirmButton } from "@boompi/ui/components/confirm-button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { useBoompi } from "@boompi/ui/transport";
import { useEffect, useMemo, useReducer } from "react";

/** All units spelled out: "2 days 5 hr 42 min" / "5 hr 42 min" / "42 min". */
function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const parts: string[] = [];
  if (d > 0) parts.push(`${d} ${d === 1 ? "day" : "days"}`);
  if (d > 0 || h > 0) parts.push(`${h} hr`);
  parts.push(`${m} min`);
  return parts.join(" ");
}

/** Live uptime in seconds. `uptime_secs` is a snapshot taken when the
 *  hello handshake fired; add the wall-clock time elapsed since it
 *  arrived and re-render on a 30s tick. A reconnect (e.g. after the box
 *  reboots) delivers a fresh hello object, resetting the baseline. */
function useLiveUptimeSecs(uptimeSnapshot: number | undefined): number | null {
  const receivedAt = useMemo(() => Date.now(), [uptimeSnapshot]);
  const [, tick] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    const t = setInterval(tick, 30_000);
    return () => clearInterval(t);
  }, []);
  if (uptimeSnapshot == null) return null;
  return uptimeSnapshot + Math.floor((Date.now() - receivedAt) / 1000);
}

/** Box health (uptime, CPU temperature + live throttle state, when the
 *  box reports them) and the restart control. */
export function SystemSection() {
  const { hello, state, send } = useBoompi();
  const diag = state?.diag;
  const hot = (diag?.cpu_temp_c ?? 0) >= 75;
  const uptimeSecs = useLiveUptimeSecs(hello?.uptime_secs);

  return (
    <Card>
      <CardHeader>
        <CardTitle>System</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {uptimeSecs != null && (
          <div className="flex items-center justify-between gap-3">
            <span>Uptime</span>
            <span className="text-muted-foreground">
              {formatUptime(uptimeSecs)}
            </span>
          </div>
        )}
        {diag?.cpu_temp_c != null && (
          <div className="flex items-center justify-between gap-3">
            <span>CPU temperature</span>
            <span className={hot ? "text-destructive" : "text-muted-foreground"}>
              {diag.cpu_temp_c.toFixed(1)} °C
            </span>
          </div>
        )}
        {diag?.throttled && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm text-muted-foreground">
              The firmware is limiting the CPU clock right now
              (overheating or a sagging power supply) - audio and games
              may stutter.
            </span>
            <Badge variant="destructive">throttled</Badge>
          </div>
        )}
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm text-muted-foreground">
            Orderly reboot - takes about half a minute.
          </span>
          <ConfirmButton
            variant="destructive"
            title="Restart the speaker?"
            description="Music stops and it's back in about 30 seconds."
            confirmLabel="Restart"
            onConfirm={() => send({ type: "reboot" })}
          >
            Restart speaker
          </ConfirmButton>
        </div>
      </CardContent>
    </Card>
  );
}
