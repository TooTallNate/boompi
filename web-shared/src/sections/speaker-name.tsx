import { useState } from "react";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function SpeakerNameSection() {
  const { state } = useBoompi();
  const { status, save } = useSave();
  const [name, setName] = useState(state?.settings.name ?? "");
  const trimmed = name.trim();
  const dirty = trimmed.length > 0 && trimmed !== state?.settings.name;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Speaker name</CardTitle>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="name">Name</FieldLabel>
            <Input
              id="name"
              maxLength={48}
              autoComplete="off"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && dirty) save({ name: trimmed });
              }}
            />
            <FieldDescription>
              Shown for Bluetooth, AirPlay, and Spotify Connect
            </FieldDescription>
          </Field>
          <div className="flex items-center gap-3">
            <Button
              disabled={!dirty || status.kind === "saving"}
              onClick={() => save({ name: trimmed })}
            >
              Save
            </Button>
            <StatusText status={status} />
          </div>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
