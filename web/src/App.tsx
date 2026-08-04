import { useState } from "react";
import type { ReactNode } from "react";
import { patchSettings } from "./api";
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

        <Section title="Wi-Fi">
          <p className="text-sm text-dim">Coming soon.</p>
        </Section>
        <Section title="Clock & timezone">
          <p className="text-sm text-dim">Coming soon.</p>
        </Section>
      </main>
    </div>
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
      {pairing.state === "idle" && (
        <button
          className="mb-2 rounded-lg bg-accent px-5 py-2.5 text-[15px] font-semibold text-accent-ink"
          onClick={() => send({ type: "pairing", action: "enable" })}
        >
          Pair a device
        </button>
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
            Confirm this code matches:
          </p>
          <p className="my-2 text-center font-mono text-2xl tracking-[0.3em]">
            {String(pairing.passkey ?? 0).padStart(6, "0")}
          </p>
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
