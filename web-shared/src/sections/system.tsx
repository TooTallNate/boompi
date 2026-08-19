import { Badge } from "@boompi/ui/components/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { useBoompi } from "@boompi/ui/transport";

/** Box health: CPU temperature + live throttle state. Hidden when the
 *  box reports no thermal data (old software / desktop sim). */
export function SystemSection() {
  const { state } = useBoompi();
  const diag = state?.diag;
  if (diag?.cpu_temp_c == null) return null;

  const hot = diag.cpu_temp_c >= 75;
  return (
    <Card>
      <CardHeader>
        <CardTitle>System</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3">
          <span>CPU temperature</span>
          <span className={hot ? "text-destructive" : "text-muted-foreground"}>
            {diag.cpu_temp_c.toFixed(1)} °C
          </span>
        </div>
        {diag.throttled && (
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm text-muted-foreground">
              The firmware is limiting the CPU clock right now
              (overheating or a sagging power supply) - audio and games
              may stutter.
            </span>
            <Badge variant="destructive">throttled</Badge>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
