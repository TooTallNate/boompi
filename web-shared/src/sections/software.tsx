import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Separator } from "@boompi/ui/components/separator";
import { Switch } from "@boompi/ui/components/switch";
import { StatusText } from "@boompi/ui/components/status-text";
import { Progress } from "@boompi/ui/components/progress";
import { describeUpdate } from "@boompi/ui/lib/update-progress";
import { cn } from "@boompi/ui/lib/utils";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function SoftwareSection() {
  const { state, send } = useBoompi();
  const { status, save } = useSave();
  const updates = state?.updates;
  const settings = state?.settings;
  if (!updates || !settings) return null;

  const progress = describeUpdate(updates);
  const detail = progress.active
    ? `Installing ${progress.version}`
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
        {progress.active && <UpdateProgress desc={progress} />}
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

function UpdateProgress({ desc }: { desc: ReturnType<typeof describeUpdate> }) {
  const details = [desc.bytesLine, desc.rateLine, desc.etaLine].filter(Boolean);
  return (
    <div className="flex flex-col gap-2" aria-live="polite">
      <div className="flex items-baseline justify-between gap-3 text-sm">
        <span className="min-w-0 truncate">{desc.stageLabel}</span>
        {desc.percent != null && (
          <span className="flex-none font-medium tabular-nums">{desc.percent}%</span>
        )}
      </div>
      <Progress
        value={desc.percent}
        aria-label={desc.stageLabel}
        className={cn(desc.percent == null && "animate-pulse")}
        indicatorClassName={cn(desc.stalled && "bg-muted-foreground")}
      />
      {details.length > 0 && (
        <div className="text-xs text-muted-foreground tabular-nums">{details.join(" · ")}</div>
      )}
    </div>
  );
}
