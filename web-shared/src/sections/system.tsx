import { Badge } from "@boompi/ui/components/badge";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { useBoompi } from "@boompi/ui/transport";

/** "3 d 4 h" / "2 h 15 min" / "42 min" (as-of-connect snapshot). */
function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d} d ${h} h`;
  if (h > 0) return `${h} h ${m} min`;
  return `${m} min`;
}

/** Box health (uptime, CPU temperature + live throttle state, when the
 *  box reports them) and the restart control. */
export function SystemSection() {
  const { hello, state, send } = useBoompi();
  const diag = state?.diag;
  const hot = (diag?.cpu_temp_c ?? 0) >= 75;

  return (
    <Card>
      <CardHeader>
        <CardTitle>System</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {hello != null && (
          <div className="flex items-center justify-between gap-3">
            <span>Uptime</span>
            <span className="text-muted-foreground">
              {formatUptime(hello.uptime_secs)}
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
          <Button
            variant="destructive"
            onClick={() => {
              if (confirm("Restart the speaker? Music stops and it's back in about 30 seconds.")) {
                send({ type: "reboot" });
              }
            }}
          >
            Restart speaker
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
