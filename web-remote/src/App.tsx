import { useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "@boompi/ui/components/alert";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Spinner } from "@boompi/ui/components/spinner";
import { SETTINGS_PAGES, SettingsShell } from "@boompi/ui/shell";
import { BoompiContext } from "@boompi/ui/transport";
import { Bluetooth, BluetoothOff } from "lucide-react";
import { BleLink } from "./ble";
import { useBoompiBle } from "./useBoompiBle";

export default function App() {
  const { conn, status, connect, disconnect } = useBoompiBle();
  const [page, setPage] = useState("general");

  if (status !== "connected" || !conn.state) {
    return <Landing status={status} error={conn.error} onConnect={connect} />;
  }

  return (
    <BoompiContext.Provider value={conn}>
      <SettingsShell
        pages={SETTINGS_PAGES}
        active={page}
        onNavigate={setPage}
        headerExtra={
          <Button variant="outline" size="sm" onClick={disconnect}>
            <BluetoothOff data-icon="inline-start" />
            Disconnect
          </Button>
        }
      />
    </BoompiContext.Provider>
  );
}

function Landing({
  status,
  error,
  onConnect,
}: {
  status: string;
  error: string | null;
  onConnect: () => Promise<void>;
}) {
  const supported = BleLink.supported();
  return (
    <div className="flex min-h-svh items-center justify-center px-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Bluetooth className="size-5 text-primary" aria-hidden />
            Boompi Remote
          </CardTitle>
          <CardDescription>
            Control a Boompi speaker over Bluetooth - no shared Wi-Fi, no
            setup. The speaker advertises its control service continuously;
            your browser finds it nearby.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {supported ? (
            <Button
              size="lg"
              disabled={status === "connecting"}
              onClick={() => void onConnect().catch(() => {})}
            >
              {status === "connecting" ? (
                <>
                  <Spinner data-icon="inline-start" />
                  Connecting…
                </>
              ) : (
                <>
                  <Bluetooth data-icon="inline-start" />
                  Find my Boompi
                </>
              )}
            </Button>
          ) : (
            <Alert>
              <AlertTitle>Web Bluetooth unavailable</AlertTitle>
              <AlertDescription>
                This browser doesn't support Web Bluetooth. Use Chrome or Edge
                on desktop/Android. (iOS Safari doesn't support it - the native
                Boompi app is coming for that.)
              </AlertDescription>
            </Alert>
          )}
          {status === "lost" && (
            <Alert>
              <AlertTitle>Bluetooth link lost</AlertTitle>
              <AlertDescription>
                The speaker went out of range or powered off. Reconnect when
                it's nearby.
              </AlertDescription>
            </Alert>
          )}
          {error && (
            <Alert variant="destructive">
              <AlertTitle>Connection failed</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          <p className="text-xs text-muted-foreground">
            Some settings (Wi-Fi scans, ROM uploads, timezone) need an IP
            connection and unlock when you open the speaker's own settings
            page over Wi-Fi - everything else works right here over the
            radio.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
