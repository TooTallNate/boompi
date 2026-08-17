import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { cn } from "@boompi/ui/lib/utils";
import { formatDuration, useBoompi } from "@boompi/ui/transport";

export function BatterySection() {
  const { state } = useBoompi();
  if (!state) return null;
  const battery = state.battery;
  const status = state.battery_status ?? "ok";
  const detail = state.battery_status_detail;

  if (!battery) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Battery</CardTitle>
        </CardHeader>
        <CardContent>
          {status === "error" ? (
            <>
              <p className="text-sm text-destructive">
                Battery sensor not responding.
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                The configured INA260 didn't answer - check the wiring and the
                bus/address in the box profile (
                <code>/data/box/hardware.toml</code>).
                {detail && <> Detail: {detail}</>}
              </p>
            </>
          ) : (
            <p className="text-xs text-muted-foreground">
              Battery monitoring isn't configured. If this box has an INA260
              power sensor, describe it in the box profile (
              <code>/data/box/hardware.toml</code>):{" "}
              <code>[battery] i2c_bus = 1, address = 0x40</code>
            </p>
          )}
        </CardContent>
      </Card>
    );
  }

  const pct = Math.round(battery.percentage * 100);
  const statusText = battery.full
    ? "Full"
    : battery.charging
      ? "Charging"
      : battery.low
        ? `Low battery${battery.time_remaining_secs != null ? ` — ${formatDuration(battery.time_remaining_secs)} left` : ""} — plug in soon`
        : battery.time_remaining_secs != null
          ? `${formatDuration(battery.time_remaining_secs)} remaining`
          : "On battery";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Battery</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <div className="flex items-baseline justify-between">
          <span className="text-[15px]">
            {pct}%{" "}
            <span
              className={cn(
                "text-muted-foreground",
                (battery.charging || battery.full) && "text-success",
                battery.low && "text-destructive",
              )}
            >
              {(battery.charging || battery.full) && "⚡ "}
              {statusText}
            </span>
          </span>
          <span className="text-xs text-muted-foreground">
            {battery.voltage.toFixed(2)} V · {battery.current >= 0 ? "+" : ""}
            {battery.current.toFixed(2)} A · {battery.power.toFixed(1)} W
          </span>
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-muted">
          <div
            className={cn("h-full rounded-full", battery.low ? "bg-destructive" : "bg-success")}
            style={{ width: `${pct}%` }}
          />
        </div>
      </CardContent>
    </Card>
  );
}
