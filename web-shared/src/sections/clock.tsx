import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@boompi/ui/components/select";
import { Separator } from "@boompi/ui/components/separator";
import { Switch } from "@boompi/ui/components/switch";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave, type ClockStatus } from "@boompi/ui/transport";

export function ClockSection() {
  const { state, rest } = useBoompi();
  const { status: fmtStatus, save } = useSave();
  const [clock, setClock] = useState<ClockStatus | null>(null);
  const [offset, setOffset] = useState(0); // device clock − browser clock
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState<string | null>(null);
  const settings = state?.settings;

  useEffect(() => {
    if (!rest) return;
    rest
      .fetchClock()
      .then((c) => {
        setClock(c);
        setOffset(c.now_ms - Date.now());
      })
      .catch((e) => setError(String(e)));
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [rest]);

  async function apply(patch: { timezone?: string; ntp?: boolean }) {
    if (!rest) return;
    setError(null);
    try {
      const c = await rest.patchClock(patch);
      setClock(c);
      setOffset(c.now_ms - Date.now());
    } catch (e) {
      setError(String(e));
    }
  }

  const deviceTime = clock
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "medium",
        timeZone: clock.timezone || undefined,
      }).format(new Date(now + offset))
    : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Clock &amp; timezone</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {settings && (
          <Field orientation="horizontal">
            <div className="flex flex-col gap-1">
              <FieldLabel htmlFor="clock-24h">24-hour clock</FieldLabel>
              <FieldDescription>
                Footer and screensaver time format (AM/PM when off)
              </FieldDescription>
            </div>
            <div className="flex items-center gap-2">
              <StatusText status={fmtStatus} />
              <Switch
                id="clock-24h"
                checked={settings.clock_24h}
                onCheckedChange={(v) => save({ clock_24h: v })}
              />
            </div>
          </Field>
        )}
        {rest && (
          <>
            <Separator />
            {!clock ? (
              <p className="text-sm text-muted-foreground">{error ?? "loading…"}</p>
            ) : (
              <>
                <div className="flex items-baseline justify-between gap-3">
                  <span className="text-lg tabular-nums">{deviceTime}</span>
                  <span className="text-xs text-muted-foreground">
                    {clock.synchronized ? "NTP synced" : "not synced"}
                  </span>
                </div>
                <Field orientation="horizontal">
                  <FieldLabel>Timezone</FieldLabel>
                  <Select
                    value={clock.timezone}
                    onValueChange={(tz) => apply({ timezone: tz })}
                  >
                    <SelectTrigger className="max-w-60">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectGroup>
                        {!clock.timezones.includes(clock.timezone) && (
                          <SelectItem value={clock.timezone}>{clock.timezone}</SelectItem>
                        )}
                        {clock.timezones.map((tz) => (
                          <SelectItem key={tz} value={tz}>
                            {tz}
                          </SelectItem>
                        ))}
                      </SelectGroup>
                    </SelectContent>
                  </Select>
                </Field>
                <Field orientation="horizontal">
                  <FieldLabel htmlFor="clock-ntp">Set time automatically (NTP)</FieldLabel>
                  <Switch
                    id="clock-ntp"
                    checked={clock.ntp}
                    onCheckedChange={(v) => apply({ ntp: v })}
                  />
                </Field>
                {error && <p className="text-xs text-destructive">{error}</p>}
              </>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
