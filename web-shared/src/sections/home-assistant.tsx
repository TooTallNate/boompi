import { useState } from "react";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldGroup, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

export function HomeAssistantSection() {
  const { state } = useBoompi();
  const { status, save } = useSave();
  const settings = state?.settings;
  // null = untouched (same greeting-races-mount fix as SpeakerName).
  const [editedBroker, setEditedBroker] = useState<string | null>(null);
  const [editedUsername, setEditedUsername] = useState<string | null>(null);
  const [editedPassword, setEditedPassword] = useState<string | null>(null);
  if (!settings) return null;
  const broker = editedBroker ?? settings.mqtt_broker;
  const username = editedUsername ?? settings.mqtt_username;
  const password = editedPassword ?? settings.mqtt_password;
  const dirty =
    broker !== settings.mqtt_broker ||
    username !== settings.mqtt_username ||
    password !== settings.mqtt_password;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Home Assistant</CardTitle>
        <CardDescription>
          Point the speaker at your MQTT broker and it appears in Home
          Assistant automatically (MQTT discovery): playback, volume, battery
          graphs, pairing, and OS updates - installable straight from HA's
          update dashboard. Leave the broker empty to disable.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="mqtt-broker">Broker</FieldLabel>
            <Input
              id="mqtt-broker"
              placeholder="e.g. 192.168.1.89:1883"
              value={broker}
              onChange={(e) => setEditedBroker(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="mqtt-username">Username</FieldLabel>
            <Input
              id="mqtt-username"
              value={username}
              onChange={(e) => setEditedUsername(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="mqtt-password">Password</FieldLabel>
            <Input
              id="mqtt-password"
              type="password"
              value={password}
              onChange={(e) => setEditedPassword(e.target.value)}
            />
          </Field>
          <div className="flex items-center justify-end gap-2">
            <StatusText status={status} />
            <Button
              disabled={!dirty}
              onClick={() =>
                save({
                  mqtt_broker: broker,
                  mqtt_username: username,
                  mqtt_password: password,
                })
              }
            >
              Save
            </Button>
          </div>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
