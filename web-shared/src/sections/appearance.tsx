import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@boompi/ui/components/select";
import { Slider } from "@boompi/ui/components/slider";
import { ToggleGroup, ToggleGroupItem } from "@boompi/ui/components/toggle-group";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function AppearanceSection() {
  const { state } = useBoompi();
  const { status, save } = useSave();
  const settings = state?.settings;
  if (!settings) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Appearance</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <Field orientation="horizontal">
          <FieldLabel>Panel theme</FieldLabel>
          <ToggleGroup
            type="single"
            variant="outline"
            value={settings.theme}
            onValueChange={(t) => t && save({ theme: t as "dark" | "light" })}
          >
            <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
            <ToggleGroupItem value="light">Light</ToggleGroupItem>
          </ToggleGroup>
        </Field>
        <Field orientation="horizontal">
          <FieldLabel>Panel text size</FieldLabel>
          <Select
            value={String(settings.ui_scale || 1)}
            onValueChange={(s) => save({ ui_scale: Number(s) })}
          >
            <SelectTrigger className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {[1, 1.25, 1.5, 1.75, 2, 2.25, 2.5].map((s) => (
                  <SelectItem key={s} value={String(s)}>
                    {Math.round(s * 100)}%
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel>
            Visualizer opacity: {Math.round(settings.visualizer_opacity * 100)}%
          </FieldLabel>
          <Slider
            min={10}
            max={100}
            value={[Math.round(settings.visualizer_opacity * 100)]}
            onValueCommit={([v]) => save({ visualizer_opacity: v / 100 })}
          />
          <FieldDescription>
            How strongly the spectrum shows behind album art on the panel
          </FieldDescription>
        </Field>
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
