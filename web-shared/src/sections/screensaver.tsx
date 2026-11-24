import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@boompi/ui/components/select";
import { ToggleGroup, ToggleGroupItem } from "@boompi/ui/components/toggle-group";
import { StatusText } from "@boompi/ui/components/status-text";
import { capsOf } from "@boompi/ui/proto";
import { useBoompi, useSave } from "@boompi/ui/transport";
import type { ScreensaverFont, ScreensaverKind } from "@boompi/ui/proto";

const KINDS: { label: string; value: ScreensaverKind }[] = [
  { label: "Off", value: "off" },
  { label: "Clock", value: "clock" },
  { label: "Matrix rain", value: "matrix" },
  { label: "Album art", value: "art" },
];

const FONTS: { label: string; value: ScreensaverFont }[] = [
  { label: "Geist", value: "geist" },
  { label: "Geist Pixel Square", value: "pixel_square" },
  { label: "Geist Pixel Grid", value: "pixel_grid" },
  { label: "Geist Pixel Circle", value: "pixel_circle" },
  { label: "Geist Pixel Triangle", value: "pixel_triangle" },
  { label: "Geist Pixel Line", value: "pixel_line" },
];

export function ScreensaverSection() {
  const { state, send, hello } = useBoompi();
  const { status, save } = useSave();
  const settings = state?.settings;
  if (!settings || !capsOf(hello).has("screensaver")) return null;
  const hasFont = capsOf(hello).has("screensaver_font");

  return (
    <Card>
      <CardHeader>
        <CardTitle>Screensaver</CardTitle>
        <CardDescription>
          Mostly-black moving content after the speaker sits idle - protects
          the panel from burn-in. Playback or a tap wakes the screen.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <ToggleGroup
          type="single"
          variant="outline"
          value={settings.screensaver}
          onValueChange={(v) => v && save({ screensaver: v as ScreensaverKind })}
        >
          {KINDS.map((k) => (
            <ToggleGroupItem key={k.value} value={k.value}>
              {k.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {settings.screensaver !== "off" && (
          <div className="flex items-center gap-3">
            <Button variant="outline" onClick={() => send({ type: "preview_screensaver" })}>
              Preview on speaker
            </Button>
            <span className="ml-auto text-sm">Start after</span>
            <Select
              value={String(settings.screensaver_min)}
              onValueChange={(m) => save({ screensaver_min: Number(m) })}
            >
              <SelectTrigger className="w-24">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {[2, 5, 10, 20, 30, 60].map((m) => (
                    <SelectItem key={m} value={String(m)}>
                      {m} min
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>
        )}
        {settings.screensaver !== "off" && hasFont && (
          <div className="flex items-center gap-3">
            <span className="text-sm">Font</span>
            <Select
              value={settings.screensaver_font ?? "geist"}
              onValueChange={(f) => save({ screensaver_font: f as ScreensaverFont })}
            >
              <SelectTrigger className="ml-auto w-52">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {FONTS.map((f) => (
                    <SelectItem key={f.value} value={f.value}>
                      {f.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </div>
        )}
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
