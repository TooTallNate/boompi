// First-boot setup wizard: name -> Wi-Fi -> done.

import { useState } from "react";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { WifiSection } from "@boompi/ui/sections/wifi";
import { SPEAKER_NAME_MAX_BYTES, utf8Bytes } from "@boompi/ui/proto";
import { sendCommand } from "./api";

export function SetupWizard({ currentName }: { currentName: string }) {
  const [step, setStep] = useState<"name" | "wifi" | "done">("name");
  const [name, setName] = useState(currentName);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const trimmed = name.trim();

  async function submitName() {
    setBusy(true);
    setError(null);
    try {
      await sendCommand({ type: "setup", speaker_name: trimmed });
      setStep("wifi");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function finish() {
    setBusy(true);
    setError(null);
    // Show the terminal step *before* the request settles: when this page
    // is served over the setup hotspot, finishing tears the hotspot down
    // and the HTTP response never arrives - which looks like a hang or an
    // error even though setup completed. (The server treats the command
    // as idempotent, so a retry after a real failure is also fine.)
    setStep("done");
    try {
      // On networks that survive (Ethernet / home Wi-Fi), the setup
      // broadcast flips `required` and this wizard unmounts into the
      // regular settings page moments later.
      await sendCommand({ type: "setup", complete: true });
    } catch {
      /* expected when the hotspot drops out from under us */
    }
  }

  return (
    <div className="flex justify-center px-4 pt-10 pb-16">
      <main className="flex w-full max-w-lg flex-col gap-4">
        <div className="flex flex-col gap-2">
          <img src="/logo.png" alt="Boompi" className="max-w-72 self-center" />
          <h1 className="text-[26px] font-semibold">Welcome 👋</h1>
          <p className="text-sm text-muted-foreground">
            Let's set up your speaker - takes about a minute.
          </p>
        </div>

        {step === "name" && (
          <Card>
            <CardHeader>
              <CardTitle>Step 1 of 2 - Name your speaker</CardTitle>
            </CardHeader>
            <CardContent>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="setup-name">Name</FieldLabel>
                  <Input
                    id="setup-name"
                    autoFocus
                    autoComplete="off"
                    placeholder="e.g. Porch Box"
                    value={name}
                    onChange={(e) => {
                      if (utf8Bytes(e.target.value.trim()) <= SPEAKER_NAME_MAX_BYTES) {
                        setName(e.target.value);
                      }
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && trimmed) submitName();
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
                  <Button disabled={!trimmed || busy} onClick={submitName}>
                    Continue
                  </Button>
                  {error && <span className="text-xs text-destructive">{error}</span>}
                </div>
              </FieldGroup>
            </CardContent>
          </Card>
        )}

        {step === "wifi" && (
          <>
            <Card>
              <CardHeader>
                <CardTitle>Step 2 of 2 - Wi-Fi (optional)</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-sm text-muted-foreground">
                  Connecting "{trimmed}" to your Wi-Fi enables Spotify Connect,
                  AirPlay, and online album art. You can skip this and set it
                  up later.
                </p>
              </CardContent>
            </Card>
            <WifiSection />
            <div className="flex items-center gap-3">
              <Button disabled={busy} onClick={finish}>
                Finish setup
              </Button>
              {error && <span className="text-xs text-destructive">{error}</span>}
            </div>
          </>
        )}

        {step === "done" && (
          <Card>
            <CardHeader>
              <CardTitle>Setup complete 🎉</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-2 text-sm text-muted-foreground">
              <p>
                "{trimmed}" is ready. If you were connected to the speaker's
                setup hotspot, it has switched off - rejoin your normal Wi-Fi
                network.
              </p>
              <p>
                If the speaker's screen still shows the setup message, reload
                this page and press "Finish setup" again.
              </p>
            </CardContent>
          </Card>
        )}
      </main>
    </div>
  );
}
