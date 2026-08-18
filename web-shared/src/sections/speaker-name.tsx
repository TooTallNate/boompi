import { useState } from "react";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { StatusText } from "@boompi/ui/components/status-text";
import { SPEAKER_NAME_MAX_BYTES, utf8Bytes } from "@boompi/ui/proto";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function SpeakerNameSection() {
  const { state } = useBoompi();
  const { status, save } = useSave();
  // null = untouched: the server value shows through, including when
  // it arrives *after* first render (the WS greeting races the mount).
  const [edited, setEdited] = useState<string | null>(null);
  const serverName = state?.settings.name ?? "";
  const name = edited ?? serverName;
  const trimmed = name.trim();
  const dirty = trimmed.length > 0 && trimmed !== serverName;

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
              autoComplete="off"
              value={name}
              onChange={(e) => {
                // Byte-capped: the Bluetooth advert has a hard 29-byte
                // budget and the 🎛️ prefix takes 8. Reject edits past
                // the cap (emoji are up to 4 bytes each).
                if (utf8Bytes(e.target.value.trim()) <= SPEAKER_NAME_MAX_BYTES) {
                  setEdited(e.target.value);
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && dirty) save({ name: trimmed });
              }}
            />
            <FieldDescription>
              Shown for Bluetooth, AirPlay, and Spotify Connect ·{" "}
              <span className={utf8Bytes(trimmed) >= SPEAKER_NAME_MAX_BYTES ? "text-destructive" : ""}>
                {utf8Bytes(trimmed)}/{SPEAKER_NAME_MAX_BYTES} bytes
              </span>
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
