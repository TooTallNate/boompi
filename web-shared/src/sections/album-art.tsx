import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldLabel } from "@boompi/ui/components/field";
import { Switch } from "@boompi/ui/components/switch";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function AlbumArtSection() {
  const { state } = useBoompi();
  const { status, save } = useSave();
  const settings = state?.settings;
  if (!settings) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Album art</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <Field orientation="horizontal">
          <FieldLabel htmlFor="art-fallback">
            Online lookup when a source sends no art
          </FieldLabel>
          <Switch
            id="art-fallback"
            checked={settings.online_art_fallback}
            onCheckedChange={(v) => save({ online_art_fallback: v })}
          />
        </Field>
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
