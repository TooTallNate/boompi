import { Alert, AlertDescription, AlertTitle } from "@boompi/ui/components/alert";
import { Badge } from "@boompi/ui/components/badge";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@boompi/ui/components/empty";
import { Separator } from "@boompi/ui/components/separator";
import { useBoompi } from "@boompi/ui/transport";
import { Bluetooth } from "lucide-react";
import { Fragment } from "react";

export function BluetoothSection() {
  const { state, send } = useBoompi();
  if (!state) return null;
  const pairing = state.pairing;
  const devices = state.bt_devices ?? [];
  const speakerName = state.settings?.name ?? "this speaker";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Bluetooth</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {(pairing.state === "idle" || pairing.state === "unavailable") && (
          <div>
            <Button onClick={() => send({ type: "pairing", action: "enable" })}>
              Pair a device
            </Button>
          </div>
        )}
        {pairing.state === "unavailable" && (
          <Alert variant="destructive">
            <AlertTitle>Bluetooth is unavailable</AlertTitle>
            <AlertDescription>
              No adapter was found. Check that the Bluetooth dongle is
              plugged in.
            </AlertDescription>
          </Alert>
        )}
        {pairing.state === "discoverable" && (
          <Alert>
            <AlertTitle>Discoverable</AlertTitle>
            <AlertDescription>
              <div className="flex w-full items-center justify-between gap-3">
                <span>
                  Choose “{speakerName}” in your device's Bluetooth settings.
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => send({ type: "pairing", action: "cancel" })}
                >
                  Cancel
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}
        {pairing.state === "pairing" && (
          <Alert>
            <AlertTitle>
              Pairing {pairing.device_name ?? "gamepad"}…
            </AlertTitle>
          </Alert>
        )}
        {pairing.state === "confirm" && (
          <Alert>
            <AlertTitle>
              Pair with {pairing.device_name ?? "device"}?
            </AlertTitle>
            <AlertDescription>
              {pairing.passkey != null && (
                <>
                  <p>Confirm this code matches:</p>
                  <p className="my-2 w-full text-center font-mono text-2xl tracking-[0.3em]">
                    {String(pairing.passkey).padStart(6, "0")}
                  </p>
                </>
              )}
              <div className="flex w-full justify-center gap-3">
                <Button onClick={() => send({ type: "pairing", action: "confirm" })}>
                  Pair
                </Button>
                <Button
                  variant="outline"
                  onClick={() => send({ type: "pairing", action: "reject" })}
                >
                  Reject
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}

        {devices.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Bluetooth />
              </EmptyMedia>
              <EmptyTitle>No paired devices</EmptyTitle>
              <EmptyDescription>
                Pair a phone for music or a gamepad for games.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          devices.map((d, i) => (
            <Fragment key={d.address}>
              {i > 0 && <Separator />}
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate">{d.name}</span>
                    {d.connected && <Badge variant="secondary">connected</Badge>}
                  </div>
                  <div className="font-mono text-xs text-muted-foreground">
                    {d.address}
                  </div>
                </div>
                <div className="flex flex-none gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      send({
                        type: "bt_device",
                        address: d.address,
                        action: d.connected ? "disconnect" : "connect",
                      })
                    }
                  >
                    {d.connected ? "Disconnect" : "Connect"}
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => {
                      if (confirm(`Unpair “${d.name}”?`)) {
                        send({
                          type: "bt_device",
                          address: d.address,
                          action: "remove",
                        });
                      }
                    }}
                  >
                    Remove
                  </Button>
                </div>
              </div>
            </Fragment>
          ))
        )}
      </CardContent>
    </Card>
  );
}
