import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  fetchClock,
  fetchWifi,
  patchClock,
  patchSettings,
  sendCommand,
  wifiAction,
} from "./api";
import type { ClockStatus, WifiNetwork, WifiStatus } from "./api";
import { useBoompi } from "./useBoompi";
import type {
  BtDevice,
  ClientMessage,
  Pairing,
  Settings,
  SettingsPatch,
  Theme,
} from "./proto";

type SaveStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "ok" }
  | { kind: "err"; message: string };

export default function App() {
  const { hello, state, error, send, applySettings } = useBoompi();
  const settings = state?.settings ?? null;

  if (state?.setup.required) {
    return (
      <SetupWizard
        currentName={settings?.name ?? ""}
        onRenamed={(name) =>
          settings && applySettings({ ...settings, name })
        }
      />
    );
  }

  return (
    <div className="flex justify-center px-4 pt-6 pb-16">
      <main className="w-full max-w-lg">
        <h1 className="mt-2 text-[22px] font-semibold">
          {settings?.name || "Boompi"}
        </h1>
        <p className="mb-6 text-[13px] text-dim">
          {error ??
            (hello
              ? `boompid v${hello.version} · up ${Math.floor(hello.uptime_secs / 60)} min`
              : "connecting…")}
        </p>

        {settings && (
          <>
            <NameSection settings={settings} onSaved={applySettings} />
            <AppearanceSection settings={settings} onSaved={applySettings} />
            <ArtSection settings={settings} onSaved={applySettings} />
            <AirplayIconSection settings={settings} onSaved={applySettings} />
          </>
        )}

        {state && (
          <BluetoothSection
            pairing={state.pairing}
            devices={state.bt_devices ?? []}
            speakerName={settings?.name ?? "this speaker"}
            send={send}
          />
        )}

        <WifiSection />
        <ClockSection />
        <DangerSection />
      </main>
    </div>
  );
}

function SignalBars({ signal }: { signal: number }) {
  const bars = signal > 75 ? 4 : signal > 50 ? 3 : signal > 25 ? 2 : 1;
  return (
    <span className="flex items-end gap-[2px]" title={`${signal}%`}>
      {[1, 2, 3, 4].map((b) => (
        <span
          key={b}
          className={`w-[3px] rounded-sm ${b <= bars ? "bg-fg" : "bg-line"}`}
          style={{ height: 3 + b * 3 }}
        />
      ))}
    </span>
  );
}

function WifiSection() {
  const [wifi, setWifi] = useState<WifiStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [joining, setJoining] = useState<string | null>(null); // ssid with open psk prompt
  const [psk, setPsk] = useState("");
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      setWifi(await fetchWifi());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 15000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function act(a: Parameters<typeof wifiAction>[0]) {
    setBusy(true);
    setError(null);
    try {
      setWifi(await wifiAction(a));
      setJoining(null);
      setPsk("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function join(net: WifiNetwork) {
    if (net.saved || net.security === "") {
      act({ action: "connect", ssid: net.ssid });
    } else {
      setJoining(net.ssid);
      setPsk("");
    }
  }

  return (
    <Section title="Wi-Fi">
      {!wifi ? (
        <p className="text-sm text-dim">{error ?? "loading…"}</p>
      ) : !wifi.supported ? (
        <p className="text-sm text-dim">No Wi-Fi hardware.</p>
      ) : (
        <>
          <div className="flex items-center justify-between gap-3 py-1.5">
            <span>
              Wi-Fi{" "}
              {wifi.connected && (
                <span className="text-[13px] text-ok">
                  — {wifi.connected}
                  {wifi.ip ? ` (${wifi.ip})` : ""}
                </span>
              )}
              {wifi.ap_active && (
                <span className="text-[13px] text-accent">
                  — setup hotspot active
                </span>
              )}
            </span>
            {/* No radio toggle while the setup hotspot is up: turning the
                radio off would kill this very connection (and the captive
                portal with it). */}
            {!wifi.ap_active && (
              <Toggle
                checked={wifi.enabled}
                onChange={(v) => act({ action: "radio", enabled: v })}
              />
            )}
          </div>

          {wifi.ap_active && (
            <p className="py-1.5 text-sm text-dim">
              You’re connected through the speaker’s setup hotspot. The
              networks below were found just before the hotspot started.
              Joining one switches the hotspot off while the speaker
              connects — rejoin your normal Wi-Fi afterwards. If the
              password is wrong, the hotspot comes back within a minute so
              you can retry. You can also skip this and set up Wi-Fi later
              from this page over your home network or Ethernet.
            </p>
          )}

          {wifi.enabled &&
            wifi.networks.map((n) => (
              <div
                key={n.ssid}
                className="border-t border-line py-2.5 first:border-t-0"
              >
                <div className="flex items-center justify-between gap-3">
                  <button
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                    onClick={() => join(n)}
                    disabled={busy || n.in_use}
                  >
                    <SignalBars signal={n.signal} />
                    <span className="truncate">{n.ssid}</span>
                    {n.security !== "" && (
                      <span className="text-[11px] text-dim">🔒</span>
                    )}
                    {n.in_use && (
                      <span className="text-[12px] text-ok">connected</span>
                    )}
                    {n.saved && !n.in_use && (
                      <span className="text-[12px] text-dim">saved</span>
                    )}
                  </button>
                  {(n.saved || n.in_use) && (
                    <button
                      className="flex-none rounded-lg border border-err/40 px-3 py-1 text-[13px] text-err hover:bg-err/10"
                      onClick={() => act({ action: "forget", name: n.ssid })}
                      disabled={busy}
                    >
                      Forget
                    </button>
                  )}
                </div>
                {joining === n.ssid && (
                  <form
                    className="mt-2 flex gap-2"
                    onSubmit={(e) => {
                      e.preventDefault();
                      act({ action: "connect", ssid: n.ssid, psk });
                    }}
                  >
                    <input
                      type="password"
                      autoFocus
                      placeholder="Password"
                      className="min-w-0 flex-1 rounded-lg border border-line bg-bg px-3 py-2 text-sm focus:border-accent focus:outline-none"
                      value={psk}
                      onChange={(e) => setPsk(e.target.value)}
                    />
                    <button
                      type="submit"
                      className="rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-accent-ink disabled:opacity-40"
                      disabled={psk.length < 8 || busy}
                    >
                      Join
                    </button>
                    <button
                      type="button"
                      className="rounded-lg border border-line px-3 py-2 text-sm text-dim"
                      onClick={() => setJoining(null)}
                    >
                      Cancel
                    </button>
                  </form>
                )}
              </div>
            ))}
          {error && <p className="mt-2 text-[13px] text-err">{error}</p>}
        </>
      )}
    </Section>
  );
}

function ClockSection() {
  const [clock, setClock] = useState<ClockStatus | null>(null);
  const [offset, setOffset] = useState(0); // device clock − browser clock
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchClock()
      .then((c) => {
        setClock(c);
        setOffset(c.now_ms - Date.now());
      })
      .catch((e) => setError(String(e)));
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  async function apply(patch: { timezone?: string; ntp?: boolean }) {
    setError(null);
    try {
      const c = await patchClock(patch);
      setClock(c);
      setOffset(c.now_ms - Date.now());
    } catch (e) {
      setError(String(e));
    }
  }

  const deviceTime = clock
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "medium",
        timeZone: clock.timezone || undefined,
      }).format(new Date(now + offset))
    : null;

  return (
    <Section title="Clock & timezone">
      {!clock ? (
        <p className="text-sm text-dim">{error ?? "loading…"}</p>
      ) : (
        <>
          <div className="mb-3 flex items-baseline justify-between gap-3">
            <span className="text-lg tabular-nums">{deviceTime}</span>
            <span className="text-[12px] text-dim">
              {clock.synchronized ? "NTP synced" : "not synced"}
            </span>
          </div>
          <div className="flex items-center justify-between gap-3 py-1.5">
            <span>Timezone</span>
            <select
              className="max-w-[240px] rounded-lg border border-line bg-bg px-2 py-2 text-sm"
              value={clock.timezone}
              onChange={(e) => apply({ timezone: e.target.value })}
            >
              {!clock.timezones.includes(clock.timezone) && (
                <option value={clock.timezone}>{clock.timezone}</option>
              )}
              {clock.timezones.map((tz) => (
                <option key={tz} value={tz}>
                  {tz}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-line py-1.5">
            <span>Set time automatically (NTP)</span>
            <Toggle checked={clock.ntp} onChange={(v) => apply({ ntp: v })} />
          </div>
          {error && <p className="mt-2 text-[13px] text-err">{error}</p>}
        </>
      )}
    </Section>
  );
}

function BluetoothSection({
  pairing,
  devices,
  speakerName,
  send,
}: {
  pairing: Pairing;
  devices: BtDevice[];
  speakerName: string;
  send: (msg: ClientMessage) => void;
}) {
  return (
    <Section title="Bluetooth">
      {(pairing.state === "idle" || pairing.state === "unavailable") && (
        <button
          className="mb-2 rounded-lg bg-accent px-5 py-2.5 text-[15px] font-semibold text-accent-ink"
          onClick={() => send({ type: "pairing", action: "enable" })}
        >
          Pair a device
        </button>
      )}
      {pairing.state === "unavailable" && (
        <div className="mb-2 rounded-lg border border-err/40 bg-err/10 p-3">
          <p className="text-sm">
            Bluetooth is unavailable — no adapter was found. Check that the
            Bluetooth dongle is plugged in.
          </p>
        </div>
      )}
      {pairing.state === "discoverable" && (
        <div className="mb-2 flex items-center justify-between gap-3 rounded-lg border border-accent/40 bg-accent/10 p-3">
          <span className="text-sm">
            Discoverable — choose “{speakerName}” in your device’s Bluetooth
            settings.
          </span>
          <button
            className="rounded-lg border border-line px-4 py-2 text-sm text-dim hover:text-fg"
            onClick={() => send({ type: "pairing", action: "cancel" })}
          >
            Cancel
          </button>
        </div>
      )}
      {pairing.state === "confirm" && (
        <div className="mb-2 rounded-lg border border-ok/40 bg-ok/10 p-3">
          <p className="text-sm">
            Pair with <strong>{pairing.device_name ?? "device"}</strong>?
            {pairing.passkey != null && " Confirm this code matches:"}
          </p>
          {pairing.passkey != null && (
            <p className="my-2 text-center font-mono text-2xl tracking-[0.3em]">
              {String(pairing.passkey).padStart(6, "0")}
            </p>
          )}
          <div className="flex justify-center gap-3">
            <button
              className="rounded-lg bg-ok px-5 py-2 text-[15px] font-semibold text-accent-ink"
              onClick={() => send({ type: "pairing", action: "confirm" })}
            >
              Pair
            </button>
            <button
              className="rounded-lg border border-line px-5 py-2 text-[15px] text-dim hover:text-fg"
              onClick={() => send({ type: "pairing", action: "reject" })}
            >
              Reject
            </button>
          </div>
        </div>
      )}

      {devices.length === 0 ? (
        <p className="text-sm text-dim">No paired devices.</p>
      ) : (
        devices.map((d) => (
          <div
            key={d.address}
            className="flex items-center justify-between gap-3 border-t border-line py-2.5 first:border-t-0"
          >
            <div className="min-w-0">
              <div className="truncate">{d.name}</div>
              <div className="text-[12px] text-dim">
                {d.connected ? (
                  <span className="text-ok">connected</span>
                ) : (
                  "not connected"
                )}
                <span className="ml-2 font-mono">{d.address}</span>
              </div>
            </div>
            <div className="flex flex-none gap-2">
              <button
                className="rounded-lg border border-line px-3 py-1.5 text-sm text-dim hover:text-fg"
                onClick={() =>
                  send({
                    type: "bt_device",
                    address: d.address,
                    action: d.connected ? "disconnect" : "connect",
                  })
                }
              >
                {d.connected ? "Disconnect" : "Connect"}
              </button>
              <button
                className="rounded-lg border border-err/40 px-3 py-1.5 text-sm text-err hover:bg-err/10"
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
              </button>
            </div>
          </div>
        ))
      )}
    </Section>
  );
}

function SetupWizard({
  currentName,
  onRenamed,
}: {
  currentName: string;
  onRenamed: (name: string) => void;
}) {
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
      onRenamed(trimmed);
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
    // and the HTTP response never arrives — which looks like a hang or an
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
      <main className="w-full max-w-lg">
        <h1 className="text-[26px] font-semibold">Welcome 👋</h1>
        <p className="mb-8 text-[14px] text-dim">
          Let’s set up your speaker — takes about a minute.
        </p>

        {step === "name" && (
          <Section title="Step 1 of 2 — Name your speaker">
            <label className="mb-1.5 block text-sm text-dim" htmlFor="setup-name">
              Shown for Bluetooth, AirPlay, and Spotify Connect
            </label>
            <input
              id="setup-name"
              type="text"
              maxLength={48}
              autoFocus
              autoComplete="off"
              placeholder="e.g. Porch Box"
              className="w-full rounded-lg border border-line bg-bg px-3 py-2.5 text-base focus:border-accent focus:outline-none"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && trimmed) submitName();
              }}
            />
            <div className="mt-3.5 flex items-center gap-3">
              <button
                className="rounded-lg bg-accent px-5 py-2.5 text-[15px] font-semibold text-accent-ink disabled:opacity-40"
                disabled={!trimmed || busy}
                onClick={submitName}
              >
                Continue
              </button>
              {error && <span className="text-[13px] text-err">{error}</span>}
            </div>
          </Section>
        )}

        {step === "wifi" && (
          <>
            <Section title="Step 2 of 2 — Wi-Fi (optional)">
              <p className="mb-2 text-sm text-dim">
                Connecting “{trimmed}” to your Wi-Fi enables Spotify Connect,
                AirPlay, and online album art. You can skip this and set it
                up later.
              </p>
            </Section>
            <WifiSection />
            <div className="flex items-center gap-3">
              <button
                className="rounded-lg bg-accent px-5 py-2.5 text-[15px] font-semibold text-accent-ink disabled:opacity-40"
                disabled={busy}
                onClick={finish}
              >
                Finish setup
              </button>
              {error && <span className="text-[13px] text-err">{error}</span>}
            </div>
          </>
        )}

        {step === "done" && (
          <Section title="Setup complete 🎉">
            <p className="text-sm text-dim">
              “{trimmed}” is ready. If you were connected to the speaker’s
              setup hotspot, it has switched off — rejoin your normal
              Wi-Fi network.
            </p>
            <p className="mt-2 text-sm text-dim">
              If the speaker’s screen still shows the setup message, reload
              this page and press “Finish setup” again.
            </p>
          </Section>
        )}
      </main>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mb-4 rounded-xl border border-line bg-panel p-[18px]">
      <h2 className="mb-3 text-sm font-medium tracking-wider text-dim uppercase">
        {title}
      </h2>
      {children}
    </section>
  );
}

interface SectionProps {
  settings: Settings;
  onSaved: (s: Settings) => void;
}

/** Shared patch-submit helper with status handling. */
function useSave(onSaved: (s: Settings) => void) {
  const [status, setStatus] = useState<SaveStatus>({ kind: "idle" });

  async function save(patch: SettingsPatch) {
    setStatus({ kind: "saving" });
    try {
      onSaved(await patchSettings(patch));
      setStatus({ kind: "ok" });
      setTimeout(() => setStatus({ kind: "idle" }), 2500);
    } catch (e) {
      setStatus({ kind: "err", message: String(e) });
    }
  }

  return { status, save };
}

function StatusText({ status }: { status: SaveStatus }) {
  switch (status.kind) {
    case "idle":
      return <span className="text-[13px]" />;
    case "saving":
      return <span className="text-[13px] text-dim">saving…</span>;
    case "ok":
      return <span className="text-[13px] text-ok">saved</span>;
    case "err":
      return <span className="text-[13px] text-err">{status.message}</span>;
  }
}

function NameSection({ settings, onSaved }: SectionProps) {
  const [name, setName] = useState(settings.name);
  const { status, save } = useSave(onSaved);
  const trimmed = name.trim();
  const dirty = trimmed.length > 0 && trimmed !== settings.name;

  return (
    <Section title="Speaker name">
      <label className="mb-1.5 block text-sm text-dim" htmlFor="name">
        Shown for Bluetooth, AirPlay, and Spotify Connect
      </label>
      <input
        id="name"
        type="text"
        maxLength={48}
        autoComplete="off"
        className="w-full rounded-lg border border-line bg-bg px-3 py-2.5 text-base focus:border-accent focus:outline-none"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && dirty) save({ name: trimmed });
        }}
      />
      <div className="mt-3.5 flex items-center gap-3">
        <button
          className="rounded-lg bg-accent px-5 py-2.5 text-[15px] font-semibold text-accent-ink disabled:cursor-default disabled:opacity-40"
          disabled={!dirty || status.kind === "saving"}
          onClick={() => save({ name: trimmed })}
        >
          Save
        </button>
        <StatusText status={status} />
      </div>
    </Section>
  );
}

function AppearanceSection({ settings, onSaved }: SectionProps) {
  const { status, save } = useSave(onSaved);
  const themes: Theme[] = ["dark", "light"];

  return (
    <Section title="Appearance">
      <div className="flex items-center justify-between gap-3 py-1.5">
        <span>Panel theme</span>
        <span className="inline-flex overflow-hidden rounded-lg border border-line">
          {themes.map((t) => (
            <button
              key={t}
              className={
                "px-4 py-2 text-sm " +
                (settings.theme === t
                  ? "bg-accent font-semibold text-accent-ink"
                  : "text-dim hover:text-fg")
              }
              onClick={() => save({ theme: t })}
            >
              {t === "dark" ? "Dark" : "Light"}
            </button>
          ))}
        </span>
      </div>
      <StatusText status={status} />
    </Section>
  );
}

function ArtSection({ settings, onSaved }: SectionProps) {
  const { status, save } = useSave(onSaved);

  return (
    <Section title="Album art">
      <div className="flex items-center justify-between gap-3 py-1.5">
        <span>Online lookup when a source sends no art</span>
        <Toggle
          checked={settings.online_art_fallback}
          onChange={(v) => save({ online_art_fallback: v })}
        />
      </div>
      <StatusText status={status} />
    </Section>
  );
}

const AIRPLAY_MODELS: { label: string; value: string }[] = [
  { label: "Generic speaker", value: "" },
  { label: "HomePod mini", value: "AudioAccessory5,1" },
  { label: "HomePod", value: "AudioAccessory1,1" },
  { label: "Apple TV", value: "AppleTV3,2" },
];

function AirplayIconSection({ settings, onSaved }: SectionProps) {
  const { status, save } = useSave(onSaved);
  const preset = AIRPLAY_MODELS.some((m) => m.value === settings.airplay_model);

  return (
    <Section title="AirPlay device icon">
      <p className="mb-2 text-sm text-dim">
        Phones choose the icon in their AirPlay list from the advertised
        model. Apple models mimic their product icons; anything else shows
        the generic speaker. Senders may need to rediscover the speaker
        (toggle Wi-Fi or wait a minute) after changing this.
      </p>
      <div className="flex items-center justify-between gap-3 py-1.5">
        <span>Advertised model</span>
        <select
          className="rounded-lg border border-line bg-bg px-3 py-2 text-sm"
          value={preset ? settings.airplay_model : "custom"}
          onChange={(e) => {
            if (e.target.value !== "custom") save({ airplay_model: e.target.value });
          }}
        >
          {AIRPLAY_MODELS.map((m) => (
            <option key={m.label} value={m.value}>
              {m.label}
            </option>
          ))}
          {!preset && <option value="custom">Custom: {settings.airplay_model}</option>}
        </select>
      </div>
      <StatusText status={status} />
    </Section>
  );
}

function DangerSection() {
  const [arm, setArm] = useState(false);
  const [busy, setBusy] = useState(false);

  async function reset() {
    setBusy(true);
    try {
      await sendCommand({ type: "factory_reset" });
    } catch {
      /* the box reboots out from under the request */
    }
  }

  return (
    <Section title="Danger zone">
      {!arm ? (
        <button
          className="rounded-lg border border-err/50 px-4 py-2 text-sm text-err hover:bg-err/10"
          onClick={() => setArm(true)}
        >
          Factory reset…
        </button>
      ) : (
        <div className="rounded-lg border border-err/40 bg-err/10 p-3">
          <p className="text-sm">
            This erases the speaker name, Wi-Fi networks, Bluetooth pairings,
            caches and settings, then reboots into first-boot setup. The OS
            itself is untouched.
          </p>
          <div className="mt-3 flex gap-3">
            <button
              className="rounded-lg bg-err px-4 py-2 text-sm font-semibold text-accent-ink disabled:opacity-40"
              disabled={busy}
              onClick={reset}
            >
              {busy ? "Resetting…" : "Erase everything & reboot"}
            </button>
            <button
              className="rounded-lg border border-line px-4 py-2 text-sm text-dim hover:text-fg"
              disabled={busy}
              onClick={() => setArm(false)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </Section>
  );
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="relative inline-flex flex-none cursor-pointer items-center">
      <input
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="h-[26px] w-11 rounded-full bg-line transition-colors peer-checked:bg-ok after:absolute after:top-[3px] after:left-[3px] after:h-5 after:w-5 after:rounded-full after:bg-fg after:transition-transform peer-checked:after:translate-x-[18px]" />
    </label>
  );
}
