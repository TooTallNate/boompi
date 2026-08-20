// Box hardware configuration (advanced): display overlays, wiring,
// provisioning. REST-only and box-app-only - never shown on the remote.

import { useEffect, useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "@boompi/ui/components/alert";
import { Button } from "@boompi/ui/components/button";
import { ConfirmButton } from "@boompi/ui/components/confirm-button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldGroup, FieldLabel } from "@boompi/ui/components/field";
import { Input } from "@boompi/ui/components/input";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@boompi/ui/components/select";
import { Textarea } from "@boompi/ui/components/textarea";
import { StatusText } from "@boompi/ui/components/status-text";
import type { SaveStatus } from "@boompi/ui/transport";
import {
  fetchBoxProfile,
  lockBoxProfile,
  putBoxProfile,
  sendCommand,
  type BoxProfile,
} from "./api";
import { tarBundle } from "./tar";

const BOX_PRESETS: Record<string, Omit<BoxProfile, "authorized_keys">> = {
  "Generic (HDMI, onboard audio + Bluetooth)": {
    config_txt: null,
    cmdline_txt: null,
    hardware_toml: null,
    env: null,
  },
  "Pi 3 + HyperPixel 4.0 + USB dongle/audio": {
    config_txt: [
      "# HyperPixel 4.0 panel + GT911 touch, rotated; the overlay also",
      "# provides the bit-banged i2c bus (i2c-11) for an INA260.",
      "dtoverlay=vc4-kms-dpi-hyperpixel4",
      "dtparam=rotate=270,touchscreen-swapped-x-y,touchscreen-inverted-x",
      "",
      "# USB Bluetooth dongle + USB audio; onboard radio/audio off.",
      "dtoverlay=disable-bt",
      "dtparam=audio=off",
    ].join("\n"),
    cmdline_txt: null,
    hardware_toml: "[battery]\ni2c_bus = 11\n\n[settings]\nui_scale = 1.5",
    env: "SLINT_KMS_ROTATION=270",
  },
  "Pi 4 + I2S DAC HAT + 1024x600 HDMI panel": {
    config_txt: [
      "# PCM51xx-family I2S DAC HAT; onboard audio off.",
      "dtoverlay=hifiberry-dac",
      "dtparam=audio=off",
      "",
      "# INA260 battery monitor on the standard I2C bus.",
      "dtparam=i2c_arm=on",
    ].join("\n"),
    cmdline_txt: "video=HDMI-A-1:1024x600M@60D",
    hardware_toml: "[battery]\ni2c_bus = 1",
    env: null,
  },
};

export function HardwarePage() {
  return (
    <div className="flex justify-center px-4 pt-6 pb-16">
      <main className="flex w-full max-w-lg flex-col gap-4">
        <p className="text-sm">
          <a className="text-muted-foreground underline hover:text-foreground" href="#">
            &larr; Back to settings
          </a>
        </p>
        <h1 className="text-[22px] font-semibold">Box hardware</h1>
        <Alert variant="destructive">
          <AlertTitle>Handle with care</AlertTitle>
          <AlertDescription>
            These settings describe this box's physical build and are written
            into the boot configuration. A wrong display overlay can leave the
            screen dark (the box stays reachable over ssh and this page); a
            wrong GPIO line can conflict with wiring. Only change them if you
            know the hardware.
          </AlertDescription>
        </Alert>
        <BoxHardwareSection />
      </main>
    </div>
  );
}

function BoxHardwareSection() {
  const [profile, setProfile] = useState<BoxProfile | "locked" | null>(null);
  const [status, setStatus] = useState<SaveStatus>({ kind: "idle" });
  const [rebootNeeded, setRebootNeeded] = useState(false);

  useEffect(() => {
    fetchBoxProfile().then(setProfile).catch(() => setProfile(null));
  }, []);

  if (profile === "locked") {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Box hardware</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            Hardware configuration is{" "}
            <span className="text-foreground">locked</span> on this box: the
            page and its API are disabled so nothing on the network can change
            the boot configuration. Administer it over ssh instead -{" "}
            <code>boompi-box</code> covers editing, applying, exporting a
            provisioning bundle, and <code>boompi-box unlock</code> to
            re-enable this page.
          </p>
        </CardContent>
      </Card>
    );
  }
  if (!profile) return null;
  const set = (patch: Partial<BoxProfile>) =>
    setProfile((p) => (p && p !== "locked" ? { ...p, ...patch } : p));

  const apply = async () => {
    setStatus({ kind: "saving" });
    try {
      const outcome = await putBoxProfile(profile);
      setStatus({ kind: "ok" });
      setRebootNeeded(outcome.firmware_changed);
    } catch (e) {
      setStatus({ kind: "err", message: (e as Error).message });
    }
  };

  const lock = async () => {
    try {
      await lockBoxProfile();
      setProfile("locked");
    } catch (e) {
      setStatus({ kind: "err", message: (e as Error).message });
    }
  };

  const download = () => {
    const files = (
      [
        ["config.txt", profile.config_txt],
        ["cmdline.txt", profile.cmdline_txt],
        ["hardware.toml", profile.hardware_toml],
        ["env", profile.env],
        ["authorized_keys", profile.authorized_keys],
      ] as const
    )
      .filter(([, v]) => v && v.trim())
      .map(([name, v]) => ({
        name: `boompi-box/${name}`,
        content: v!.trim() + "\n",
      }));
    const url = URL.createObjectURL(tarBundle(files));
    const a = document.createElement("a");
    a.href = url;
    a.download = "boompi-box.tar";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Box hardware</CardTitle>
        <CardDescription>
          This box's hardware profile (display, wiring, battery). Applied live
          to <code>/data/box/</code> and merged into the boot config; it
          survives OS updates. Download it as a bundle to provision another SD
          card (drop the extracted <code>boompi-box/</code> folder onto a
          freshly flashed card's boot partition).
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <Field>
            <FieldLabel>Preset</FieldLabel>
            <Select
              value=""
              onValueChange={(k) => {
                const p = BOX_PRESETS[k];
                if (p) {
                  setProfile({ ...p, authorized_keys: profile.authorized_keys });
                  setRebootNeeded(false);
                }
              }}
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Load a preset…" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {Object.keys(BOX_PRESETS).map((k) => (
                    <SelectItem key={k} value={k}>
                      {k}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="hw-config">
              config.txt fragment (dtoverlays, dtparams, GPIO)
            </FieldLabel>
            <Textarea
              id="hw-config"
              className="h-28 font-mono text-xs"
              value={profile.config_txt ?? ""}
              onChange={(e) => set({ config_txt: e.target.value || null })}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="hw-cmdline">
              Kernel arguments (single line; e.g. video= for an EDID-less panel)
            </FieldLabel>
            <Input
              id="hw-cmdline"
              className="font-mono text-xs"
              value={profile.cmdline_txt ?? ""}
              onChange={(e) => set({ cmdline_txt: e.target.value || null })}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="hw-toml">
              hardware.toml (battery wiring/thresholds; [settings] seeds first boot)
            </FieldLabel>
            <Textarea
              id="hw-toml"
              className="h-20 font-mono text-xs"
              value={profile.hardware_toml ?? ""}
              onChange={(e) => set({ hardware_toml: e.target.value || null })}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="hw-env">
              Panel environment (e.g. SLINT_KMS_ROTATION=270)
            </FieldLabel>
            <Textarea
              id="hw-env"
              className="h-12 font-mono text-xs"
              value={profile.env ?? ""}
              onChange={(e) => set({ env: e.target.value || null })}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="hw-keys">
              SSH authorized keys (public keys, one per line - required before
              locking; ssh is key-only)
            </FieldLabel>
            <Textarea
              id="hw-keys"
              className="h-16 font-mono text-xs"
              placeholder="ssh-ed25519 AAAA... you@laptop"
              value={profile.authorized_keys ?? ""}
              onChange={(e) => set({ authorized_keys: e.target.value || null })}
            />
          </Field>
          <div className="flex items-center gap-3">
            <ConfirmButton
              disabled={status.kind === "saving"}
              title="Apply this hardware profile?"
              description="It is written into the boot configuration of both OS slots and takes effect on reboot."
              confirmLabel="Apply"
              onConfirm={() => void apply()}
            >
              Apply to this box
            </ConfirmButton>
            <Button variant="outline" onClick={download}>
              Download bundle
            </Button>
            <ConfirmButton
              variant="destructive"
              title="Lock hardware configuration?"
              description={
                <>
                  This page and its API turn off; further changes require ssh
                  (<code>boompi-box</code>). Unlock with{" "}
                  <code>boompi-box unlock</code>.
                </>
              }
              confirmLabel="Lock"
              onConfirm={() => void lock()}
            >
              Lock
            </ConfirmButton>
            <StatusText status={status} />
          </div>
          {rebootNeeded && (
            <Alert>
              <AlertDescription>
                <div className="flex w-full items-center justify-between gap-3">
                  <span>Boot config changed - reboot to apply.</span>
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => sendCommand({ type: "reboot" })}
                  >
                    Reboot now
                  </Button>
                </div>
              </AlertDescription>
            </Alert>
          )}
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
