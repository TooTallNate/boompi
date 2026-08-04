import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { fetchState, patchSettings } from "./api";
import type { Hello, Settings, SettingsPatch, Theme } from "./proto";

type SaveStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "ok" }
  | { kind: "err"; message: string };

export default function App() {
  const [hello, setHello] = useState<Hello | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchState()
      .then((data) => {
        setHello(data.hello);
        setSettings(data.state.settings);
      })
      .catch((e) => setError(String(e)));
  }, []);

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
            <NameSection settings={settings} onSaved={setSettings} />
            <AppearanceSection settings={settings} onSaved={setSettings} />
            <ArtSection settings={settings} onSaved={setSettings} />
          </>
        )}

        <Section title="Wi-Fi">
          <p className="text-sm text-dim">Coming soon.</p>
        </Section>
        <Section title="Bluetooth devices">
          <p className="text-sm text-dim">Coming soon.</p>
        </Section>
        <Section title="Clock & timezone">
          <p className="text-sm text-dim">Coming soon.</p>
        </Section>
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
