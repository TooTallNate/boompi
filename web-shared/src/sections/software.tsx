import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Separator } from "@boompi/ui/components/separator";
import { Switch } from "@boompi/ui/components/switch";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

const STAGE_LABEL: Record<string, string> = {
  downloading_system: "downloading system",
  verifying_system: "verifying system",
  downloading_boot: "downloading boot files",
  verifying_boot: "verifying boot files",
  restarting: "restarting",
};

export function SoftwareSection() {
  const { state, send } = useBoompi();
  const { status, save } = useSave();
  const updates = state?.updates;
  const settings = state?.settings;
  if (!updates || !settings) return null;

  const detail = updates.applying
    ? `Installing ${updates.applying}: ${STAGE_LABEL[updates.stage ?? ""] ?? "preparing"}… ${Math.round((updates.progress ?? 0) * 100)}%`
    : updates.checking
      ? "Checking…"
      : updates.available
        ? `${updates.available} is available`
        : `No update available on the ${settings.update_channel} channel`;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Software update</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <div>{updates.version}</div>
            <div className="text-xs text-muted-foreground">{detail}</div>
          </div>
          <div className="flex flex-none gap-2">
            {updates.applying == null && updates.available != null && (
              <Button size="sm" onClick={() => send({ type: "update", action: "apply" })}>
                Update
              </Button>
            )}
            {/* Always allow a re-check while idle: a stored offer may have
                been superseded by a newer build (edge moves fast). */}
            {updates.applying == null && (
              <Button
                size="sm"
                variant="outline"
                disabled={updates.checking}
                onClick={() => send({ type: "update", action: "check" })}
              >
                {updates.available != null ? "Re-check" : "Check now"}
              </Button>
            )}
          </div>
        </div>
        <Separator />
        <Field orientation="horizontal">
          <div className="flex flex-col gap-1">
            <FieldLabel htmlFor="update-edge">Bleeding edge updates</FieldLabel>
            <FieldDescription>
              Follow every green dev build, not just tagged releases
            </FieldDescription>
          </div>
          <Switch
            id="update-edge"
            checked={settings.update_channel === "edge"}
            onCheckedChange={(v) => save({ update_channel: v ? "edge" : "stable" })}
          />
        </Field>
        {updates.error && <p className="text-xs text-destructive">{updates.error}</p>}
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
