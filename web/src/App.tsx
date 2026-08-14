import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  fetchBoxProfile,
  lockBoxProfile,
  fetchClock,
  fetchWifi,
  patchClock,
  patchSettings,
  putBoxProfile,
  sendCommand,
  wifiAction,
} from "./api";
import type { BoxProfile, ClockStatus, WifiNetwork, WifiStatus } from "./api";
import { tarBundle } from "./tar";
import { useBoompi } from "./useBoompi";
import type {
  Battery,
  BtDevice,
  GamesState,
  BtVolumeMode,
  ClientMessage,
  EmojiFontsState,
  Pairing,
  Settings,
  SettingsPatch,
  ScreensaverKind,
  Theme,
  UpdateState,
} from "./proto";

type SaveStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "ok" }
  | { kind: "err"; message: string };

function useHashRoute(): string {
  const [hash, setHash] = useState(window.location.hash);
  useEffect(() => {
    const onChange = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);
  return hash;
}

export default function App() {
  const { hello, state, error, send, applySettings } = useBoompi();
  const settings = state?.settings ?? null;
  const route = useHashRoute();

  if (route === "#/hardware") {
    return <HardwarePage />;
  }

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
            {state?.emoji_fonts && (
              <EmojiFontSection emoji={state.emoji_fonts} send={send} />
            )}
            <ArtSection settings={settings} onSaved={applySettings} />
            <ScreensaverSection
              settings={settings}
              onSaved={applySettings}
              send={send}
            />
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

        {state?.games && settings && (
          <GamesSection
            games={state.games}
            settings={settings}
            onSaved={applySettings}
            send={send}
          />
        )}
        {state && (
          <BatterySection
            battery={state.battery}
            status={state.battery_status ?? "ok"}
            detail={state.battery_status_detail}
          />
        )}

        <WifiSection />
        {settings && (
          <ClockSection settings={settings} onSaved={applySettings} />
        )}
        {settings && (
          <HomeAssistantSection settings={settings} onSaved={applySettings} />
        )}
        {state?.updates && settings && (
          <UpdateSection
            updates={state.updates}
            settings={settings}
            send={send}
            onSaved={applySettings}
          />
        )}
        <p className="mt-6 text-center text-[12px] text-dim">
          <a className="underline hover:text-fg" href="#/hardware">
            Box hardware configuration
          </a>{" "}
          - display, wiring, provisioning (advanced).
        </p>
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

const GAME_SYSTEMS = [
  ["nes", "NES"],
  ["snes", "SNES"],
  ["gb", "Game Boy"],
  ["gbc", "Game Boy Color"],
  ["gba", "Game Boy Advance"],
  ["n64", "Nintendo 64"],
  ["psx", "PlayStation"],
  ["bios", "BIOS files (PSX etc.)"],
] as const;

function GamesSection({
  games,
  settings,
  onSaved,
  send,
}: {
  games: GamesState;
  send: (msg: ClientMessage) => void;
} & SectionProps) {
  const [system, setSystem] = useState<string>("nes");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { save, status } = useSave(onSaved);

  const upload = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const form = new FormData();
      for (const f of Array.from(files)) form.append("file", f);
      const r = await fetch(`/api/games/upload?system=${system}`, {
        method: "POST",
        body: form,
      });
      if (!r.ok) {
        const body = await r.json().catch(() => ({}));
        throw new Error(body.error ?? `HTTP ${r.status}`);
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const del = async (system: string, file: string) => {
    if (!window.confirm(`Delete ${file}? Save files are kept.`)) return;
    await fetch("/api/games/delete", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ system, file }),
    });
  };

  const freeGB = (games.storage_free / 1e9).toFixed(1);
  return (
    <Section title="Games">
      <p className="mb-3 text-[13px] text-dim">
        RetroArch is aboard. Upload your ROMs here or drag them onto the
        network share (<code>smb://{window.location.hostname}/games</code>
        , guest access), pair a controller (same pairing button as
        speakers), launch from the panel. Music and gameplay mix; music
        ducks the game volume.
      </p>
      {games.running && (
        <div className="mb-3 flex items-center gap-3 rounded-lg border border-line bg-bg p-3">
          <span className="text-[13px]">
            Playing: <code>{games.running}</code>
          </span>
          <button
            className="rounded-lg border border-err/40 px-3 py-1 text-[13px] text-err hover:bg-err/10"
            onClick={() => send({ type: "game", action: "stop" })}
          >
            Stop game
          </button>
        </div>
      )}
      <div className="mb-3 flex items-center gap-2">
        <select
          className="rounded-lg border border-line bg-bg px-2 py-2 text-sm"
          value={system}
          onChange={(e) => setSystem(e.target.value)}
        >
          {GAME_SYSTEMS.map(([id, label]) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </select>
        <label className="cursor-pointer rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-accent-ink">
          {busy ? "Uploading…" : "Upload"}
          <input
            type="file"
            multiple
            className="hidden"
            disabled={busy}
            onChange={(e) => {
              void upload(e.target.files);
              e.target.value = "";
            }}
          />
        </label>
        <span className="text-[12px] text-dim">{freeGB}GB free</span>
      </div>
      {error && <p className="mb-2 text-[13px] text-err">{error}</p>}
      {games.games.length > 0 && (
        <ul className="mb-3">
          {games.games.map((g) => (
            <li
              key={`${g.system}/${g.file}`}
              className="flex items-center gap-3 border-t border-line py-1.5 first:border-t-0"
            >
              <span className="w-10 text-[11px] text-accent">{g.system}</span>
              <span className="min-w-0 flex-1 truncate text-sm">{g.name}</span>
              <span className="text-[11px] text-dim">
                {(g.size / 1e6).toFixed(1)}MB
              </span>
              <button
                className="text-[12px] text-dim underline hover:text-err"
                onClick={() => void del(g.system, g.file)}
              >
                delete
              </button>
            </li>
          ))}
        </ul>
      )}
      <label className="block text-[13px] text-dim">
        Game volume while music plays: {Math.round(settings.game_volume * 100)}%
        <input
          type="range"
          min={0}
          max={100}
          className="mt-1 block w-full"
          value={Math.round(settings.game_volume * 100)}
          onChange={(e) =>
            save({ game_volume: Number(e.target.value) / 100 })
          }
        />
      </label>
      <div className="mt-1">
        <StatusText status={status} />
      </div>
    </Section>
  );
}

function BatterySection({
  battery,
  status,
  detail,
}: {
  battery: Battery | null;
  status: "unconfigured" | "error" | "ok";
  detail?: string;
}) {
  if (!battery) {
    return (
      <Section title="Battery">
        {status === "error" ? (
          <>
            <p className="text-sm text-err">Battery sensor not responding.</p>
            <p className="mt-1 text-[13px] text-dim">
              The configured INA260 didn't answer - check the wiring and the
              bus/address in the box profile (
              <code>/data/box/hardware.toml</code>).
              {detail && <> Detail: {detail}</>}
            </p>
          </>
        ) : (
          <p className="text-[13px] text-dim">
            Battery monitoring isn't configured. If this box has an INA260
            power sensor, describe it in the box profile
            (<code>/data/box/hardware.toml</code>):{" "}
            <code>[battery] i2c_bus = 1, address = 0x40</code>
          </p>
        )}
      </Section>
    );
  }
  const pct = Math.round(battery.percentage * 100);
  const statusText = battery.full
    ? "Full"
    : battery.charging
      ? "Charging"
      : battery.low
        ? `Low battery${battery.time_remaining_secs != null ? ` — ${formatDuration(battery.time_remaining_secs)} left` : ""} — plug in soon`
        : battery.time_remaining_secs != null
          ? `${formatDuration(battery.time_remaining_secs)} remaining`
          : "On battery";
  const low = battery.low;
  return (
    <Section title="Battery">
      <div className="mb-2 flex items-baseline justify-between">
        <span className="text-[15px]">
          {pct}%{" "}
          <span className={battery.charging || battery.full ? "text-ok" : low ? "text-err" : "text-dim"}>
            {(battery.charging || battery.full) && "⚡ "}
            {statusText}
          </span>
        </span>
        <span className="text-[13px] text-dim">
          {battery.voltage.toFixed(2)} V · {battery.current >= 0 ? "+" : ""}
          {battery.current.toFixed(2)} A · {battery.power.toFixed(1)} W
        </span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-bg">
        <div
          className={`h-full rounded-full ${low ? "bg-err" : "bg-ok"}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </Section>
  );
}

function formatDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
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
                  - {wifi.connected}
                  {wifi.ip ? ` (${wifi.ip})` : ""}
                </span>
              )}
              {wifi.ap_active && (
                <span className="text-[13px] text-accent">
                  - setup hotspot active
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
              connects - rejoin your normal Wi-Fi afterwards. If the
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

function ClockSection({ settings, onSaved }: SectionProps) {
  const { status: fmtStatus, save } = useSave(onSaved);
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
      <div className="mb-3 flex items-center justify-between gap-3 border-b border-line pb-2.5">
        <div className="min-w-0">
          <div>24-hour clock</div>
          <div className="text-[12px] text-dim">
            Footer and screensaver time format (AM/PM when off)
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StatusText status={fmtStatus} />
          <Toggle
            checked={settings.clock_24h}
            onChange={(v) => save({ clock_24h: v })}
          />
        </div>
      </div>
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
            Bluetooth is unavailable - no adapter was found. Check that the
            Bluetooth dongle is plugged in.
          </p>
        </div>
      )}
      {pairing.state === "discoverable" && (
        <div className="mb-2 flex items-center justify-between gap-3 rounded-lg border border-accent/40 bg-accent/10 p-3">
          <span className="text-sm">
            Discoverable - choose “{speakerName}” in your device’s Bluetooth
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
      {pairing.state === "pairing" && (
        <div className="mb-2 rounded-lg border border-ok/40 bg-ok/10 p-3">
          <p className="text-sm">
            Pairing <strong>{pairing.device_name ?? "gamepad"}</strong>
            &hellip;
          </p>
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
              <label className="mt-1 flex items-center gap-2 text-[12px] text-dim">
                Volume control
                <select
                  className="rounded-md border border-line bg-transparent px-2 py-1 text-[12px] text-fg"
                  value={d.volume_mode}
                  onChange={(e) =>
                    send({
                      type: "bt_device",
                      address: d.address,
                      action: {
                        set_volume_mode: {
                          mode: e.target.value as BtVolumeMode,
                        },
                      },
                    })
                  }
                >
                  <option value="auto">
                    Auto ({d.volume_mode_auto === "phone" ? "phone" : "speaker"})
                  </option>
                  <option value="phone">Phone applies volume</option>
                  <option value="speaker">Speaker applies volume</option>
                </select>
              </label>
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
      <main className="w-full max-w-lg">
        <h1 className="text-[26px] font-semibold">Welcome 👋</h1>
        <p className="mb-8 text-[14px] text-dim">
          Let’s set up your speaker - takes about a minute.
        </p>

        {step === "name" && (
          <Section title="Step 1 of 2 - Name your speaker">
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
            <Section title="Step 2 of 2 - Wi-Fi (optional)">
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
              setup hotspot, it has switched off - rejoin your normal
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

function ScreensaverSection({
  settings,
  onSaved,
  send,
}: SectionProps & { send: (msg: ClientMessage) => void }) {
  const { status, save } = useSave(onSaved);
  const kinds: { label: string; value: ScreensaverKind }[] = [
    { label: "Off", value: "off" },
    { label: "Clock", value: "clock" },
    { label: "Matrix rain", value: "matrix" },
    { label: "Album art", value: "art" },
  ];
  return (
    <Section title="Screensaver">
      <p className="mb-2 text-sm text-dim">
        Mostly-black moving content after the speaker sits idle - protects
        the panel from burn-in. Playback or a tap wakes the screen.
      </p>
      <div className="flex flex-wrap gap-2">
        {kinds.map((k) => (
          <button
            key={k.value}
            className={`rounded-lg border px-3 py-1.5 text-sm ${
              settings.screensaver === k.value
                ? "border-accent bg-accent/10 text-fg"
                : "border-line text-dim hover:text-fg"
            }`}
            onClick={() => save({ screensaver: k.value })}
          >
            {k.label}
          </button>
        ))}
      </div>
      {settings.screensaver !== "off" && (
        <div className="mt-3 flex items-center justify-between gap-3 py-1.5">
          <button
            className="rounded-lg border border-line px-3 py-1.5 text-sm text-dim hover:text-fg"
            onClick={() => send({ type: "preview_screensaver" })}
          >
            Preview on speaker
          </button>
          <span className="ml-auto">Start after</span>
          <select
            className="rounded-lg border border-line bg-panel px-2 py-1.5 text-sm"
            value={settings.screensaver_min}
            onChange={(e) =>
              save({ screensaver_min: Number(e.target.value) })
            }
          >
            {[2, 5, 10, 20, 30, 60].map((m) => (
              <option key={m} value={m}>
                {m} min
              </option>
            ))}
          </select>
        </div>
      )}
      <StatusText status={status} />
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
      <div className="flex items-center justify-between gap-3 py-1.5">
        <span>Panel text size</span>
        <select
          className="rounded-lg border border-line bg-bg px-3 py-2 text-sm"
          value={String(settings.ui_scale || 1)}
          onChange={(e) => save({ ui_scale: Number(e.target.value) })}
        >
          {[1, 1.25, 1.5, 1.75, 2, 2.25, 2.5].map((s) => (
            <option key={s} value={String(s)}>
              {Math.round(s * 100)}%
            </option>
          ))}
        </select>
      </div>
      <StatusText status={status} />
    </Section>
  );
}

function EmojiFontSection({
  emoji,
  send,
}: {
  emoji: EmojiFontsState;
  send: (msg: ClientMessage) => void;
}) {
  // Fully push-driven: the ws State snapshot carries the catalog and
  // every change (downloads incl. progress, selection) arrives as an
  // emoji_fonts broadcast - no REST polling. The REST endpoint remains
  // for curl debugging.
  return (
    <Section title="Emoji style">
      <p className="mb-2 text-sm text-dim">
        The font used for emoji on the speaker's screen (name, track
        titles). Downloads are stored on the speaker and survive updates;
        switching restarts the panel UI briefly.
      </p>
      {emoji.fonts.map((f) => (
        <div
          key={f.id}
          className="flex items-center justify-between gap-3 border-t border-line py-2.5 first:border-t-0"
        >
          <div className="min-w-0">
            <div>
              {f.label}
              {f.active && <span className="ml-2 text-[12px] text-ok">active</span>}
            </div>
            <div className="text-[12px] text-dim">
              {f.license}
              {f.size > 0 && !f.installed && (
                <span className="ml-2">{Math.round(f.size / 1024 / 1024)} MB</span>
              )}
            </div>
          </div>
          <div className="flex flex-none gap-2">
            {f.installed && !f.active && (
              <button
                className="rounded-lg bg-accent px-3 py-1.5 text-sm font-semibold text-accent-ink"
                onClick={() => send({ type: "emoji_font", action: "select", id: f.id })}
              >
                Use
              </button>
            )}
            {!f.installed &&
              (emoji.downloading === f.id ? (
                <span className="px-3 py-1.5 text-sm text-dim">
                  Downloading… {Math.round((emoji.progress ?? 0) * 100)}%
                </span>
              ) : (
                <button
                  className="rounded-lg border border-line px-3 py-1.5 text-sm text-dim hover:text-fg disabled:opacity-40"
                  disabled={emoji.downloading != null}
                  onClick={() => send({ type: "emoji_font", action: "download", id: f.id })}
                >
                  Download
                </button>
              ))}
            {f.installed && !f.builtin && !f.active && (
              <button
                className="rounded-lg border border-err/40 px-3 py-1.5 text-sm text-err hover:bg-err/10"
                onClick={() => send({ type: "emoji_font", action: "remove", id: f.id })}
              >
                Remove
              </button>
            )}
          </div>
        </div>
      ))}
      {emoji.error && <p className="mt-2 text-[13px] text-err">{emoji.error}</p>}
    </Section>
  );
}

function UpdateSection({
  updates,
  settings,
  send,
  onSaved,
}: {
  updates: UpdateState;
  settings: Settings;
  send: (msg: ClientMessage) => void;
  onSaved: (s: Settings) => void;
}) {
  const { status, save } = useSave(onSaved);
  const stageLabel: Record<string, string> = {
    downloading_system: "downloading system",
    verifying_system: "verifying system",
    downloading_boot: "downloading boot files",
    verifying_boot: "verifying boot files",
    restarting: "restarting",
  };
  const detail = updates.applying
    ? `Installing ${updates.applying}: ${stageLabel[updates.stage ?? ""] ?? "preparing"}… ${Math.round((updates.progress ?? 0) * 100)}%`
    : updates.checking
      ? "Checking…"
      : updates.available
        ? `${updates.available} is available`
        : `No update available on the ${settings.update_channel} channel`;

  return (
    <Section title="Software update">
      <div className="flex items-center justify-between gap-3 py-1.5">
        <div className="min-w-0">
          <div>{updates.version}</div>
          <div className="text-[12px] text-dim">{detail}</div>
        </div>
        <div className="flex flex-none gap-2">
          {updates.applying == null && updates.available != null && (
            <button
              className="rounded-lg bg-accent px-3 py-1.5 text-sm font-semibold text-accent-ink"
              onClick={() => send({ type: "update", action: "apply" })}
            >
              Update
            </button>
          )}
          {/* Always allow a re-check while idle: a stored offer may have
              been superseded by a newer build (edge moves fast). */}
          {updates.applying == null && (
            <button
              className="rounded-lg border border-line px-3 py-1.5 text-sm text-dim hover:text-fg disabled:opacity-40"
              disabled={updates.checking}
              onClick={() => send({ type: "update", action: "check" })}
            >
              {updates.available != null ? "Re-check" : "Check now"}
            </button>
          )}
        </div>
      </div>
      <div className="mt-1.5 flex items-center justify-between gap-3 border-t border-line pt-2.5 pb-1.5">
        <div className="min-w-0">
          <div>Bleeding edge updates</div>
          <div className="text-[12px] text-dim">
            Follow every green dev build, not just tagged releases
          </div>
        </div>
        <Toggle
          checked={settings.update_channel === "edge"}
          onChange={(v) => save({ update_channel: v ? "edge" : "stable" })}
        />
      </div>
      {updates.error && (
        <p className="mt-2 text-[13px] text-err">{updates.error}</p>
      )}
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

// Values are mDNS `model=` strings. Senders resolve Apple model strings
// to product icons (only the HomePods and Apple TV have any; AirPort,
// Mac, iPhone and Vision models all draw the generic glyph). There are
// no non-Apple presets: the third-party icon feature bits are
// booby-trapped on current iOS - bit 26 (bookshelf icon) doubles as
// Authentication_4 and makes senders abort the handshake demanding
// MFi auth, bit 51 draws the icon but demands HomeKit PIN pairing we
// don't implement.
const AIRPLAY_MODELS: { label: string; value: string }[] = [
  { label: "Generic speaker", value: "" },
  { label: "HomePod mini", value: "AudioAccessory5,1" },
  { label: "HomePod", value: "AudioAccessory1,1" },
  { label: "Apple TV", value: "AppleTV14,1" },
];

// Approximations of the icons iOS draws for each model, matched against
// picker screenshots.
// The Apple TV badge outline+wordmark, as a single silhouette path.
const APPLE_TV_PATH =
  "M 267.285156 232.710938 L 249.253906 232.710938 L 222.746094 158.136719 L 240.238281 158.136719 L 258.710938 215.410156 L 258.988281 215.410156 L 276.773438 158.136719 L 293.367188 158.136719 Z M 214.828125 170.355469 L 200.304688 170.355469 L 200.304688 210.152344 C 200.304688 211.882813 200.40625 213.410156 200.554688 214.644531 C 200.6875 215.910156 201.011719 216.964844 201.554688 217.820313 C 202.054688 218.699219 202.816406 219.316406 203.800781 219.757813 C 204.902344 220.257813 206.3125 220.433594 208.105469 220.433594 C 209.191406 220.433594 210.320313 220.402344 211.46875 220.34375 C 212.597656 220.316406 213.699219 220.167969 214.828125 219.875 L 214.828125 232.5625 C 213.066406 232.769531 211.335938 232.914063 209.601563 233.148438 C 207.957031 233.296875 206.210938 233.386719 204.433594 233.386719 C 200.203125 233.386719 196.796875 232.972656 194.183594 232.238281 C 191.613281 231.359375 189.597656 230.152344 188.117188 228.597656 C 186.679688 226.953125 185.691406 225.015625 185.164063 222.605469 C 184.679688 220.167969 184.402344 217.496094 184.238281 214.382813 L 184.238281 170.355469 L 172.109375 170.355469 L 172.109375 158.136719 L 184.238281 158.136719 L 184.238281 135.8125 L 200.304688 135.8125 L 200.304688 158.136719 L 214.828125 158.136719 Z M 147.054688 222.699219 C 142.621094 229.171875 138.054688 235.609375 130.8125 235.742188 C 123.707031 235.875 121.414063 231.519531 113.292969 231.519531 C 105.171875 231.519531 102.632813 235.609375 95.90625 235.875 C 88.929688 236.136719 83.613281 228.890625 79.148438 222.453125 C 70.058594 209.269531 63.066406 185.214844 72.4375 168.988281 C 77.089844 160.925781 85.375 155.8125 94.378906 155.675781 C 101.25 155.558594 107.699219 160.285156 111.914063 160.285156 C 116.039063 160.285156 123.46875 154.765625 132.179688 155.402344 C 135.613281 155.667969 145.335938 156.6875 151.605469 165.953125 C 151.109375 166.265625 140.050781 172.71875 140.167969 186.148438 C 140.300781 202.207031 154.234375 207.554688 154.398438 207.613281 C 154.265625 208 152.164063 215.234375 147.054688 222.699219 Z M 116.964844 135.164063 C 120.957031 130.476563 127.734375 126.980469 133.316406 126.757813 C 134.023438 133.25 131.421875 139.808594 127.546875 144.476563 C 123.652344 149.183594 117.300781 152.835938 111.074219 152.351563 C 110.199219 145.976563 113.359375 139.316406 116.964844 135.164063 Z M 364.867188 100.128906 C 364.867188 96.484375 364.867188 92.871094 364.851563 89.199219 C 364.589844 81.300781 364.089844 73.339844 362.632813 65.496094 C 361.296875 57.539063 358.933594 50.078125 355.277344 42.910156 C 351.636719 35.804688 346.9375 29.3125 341.28125 23.671875 C 335.6875 18.09375 329.164063 13.335938 322.085938 9.722656 C 314.816406 6.050781 307.429688 3.671875 299.46875 2.289063 C 291.703125 0.851563 283.714844 0.382813 275.769531 0.175781 C 272.140625 0.117188 268.441406 0.0898438 264.828125 0 L 100.097656 0 C 96.457031 0.0898438 92.871094 0.117188 89.183594 0.175781 C 81.300781 0.382813 73.328125 0.851563 65.410156 2.289063 C 57.492188 3.671875 50.105469 6.050781 42.882813 9.722656 C 35.761719 13.335938 29.296875 18.09375 23.6875 23.671875 C 18.019531 29.3125 13.320313 35.804688 9.734375 42.910156 C 6.007813 50.078125 3.6875 57.539063 2.292969 65.496094 C 0.867188 73.339844 0.367188 81.300781 0.191406 89.199219 C 0.0742188 92.871094 0.0585938 96.484375 0 100.128906 L 0 264.8125 C 0.0585938 268.484375 0.0742188 272.097656 0.191406 275.769531 C 0.367188 283.699219 0.867188 291.65625 2.292969 299.5 C 3.6875 307.460938 6.007813 314.890625 9.734375 322.117188 C 13.320313 329.164063 18.019531 335.6875 23.6875 341.238281 C 29.296875 346.90625 35.761719 351.632813 42.882813 355.21875 C 50.105469 358.917969 57.492188 361.269531 65.410156 362.679688 C 73.328125 364.089844 81.300781 364.558594 89.183594 364.792969 C 92.871094 364.882813 96.457031 364.910156 100.097656 364.910156 C 104.429688 364.941406 108.71875 364.941406 113.066406 364.941406 L 251.992188 364.941406 C 256.234375 364.941406 260.566406 364.941406 264.828125 364.910156 C 268.441406 364.910156 272.140625 364.882813 275.769531 364.792969 C 283.714844 364.558594 291.703125 364.089844 299.46875 362.679688 C 307.429688 361.269531 314.816406 358.917969 322.085938 355.21875 C 329.164063 351.632813 335.6875 346.90625 341.28125 341.238281 C 346.9375 335.6875 351.636719 329.164063 355.277344 322.117188 C 358.933594 314.890625 361.296875 307.460938 362.632813 299.5 C 364.089844 291.65625 364.589844 283.699219 364.851563 275.769531 C 364.867188 272.097656 364.867188 268.484375 364.867188 264.8125 C 365 260.523438 365 256.265625 365 251.859375 L 365 113.078125 C 365 108.734375 365 104.414063 364.867188 100.128906";

function AirplayModelIcon({ model }: { model: string }) {
  const cls = "h-9 w-9";
  switch (model) {
    case "AudioAccessory5,1": // HomePod mini
      return (
        <svg viewBox="0 0 64 64" fill="none" className={cls} aria-hidden="true">
          <path
            fill="currentColor"
            d="M32 8C19.3 8 12 16.2 12 31.2 12 46.1 19.2 55 32 55s20-8.9 20-23.8C52 16.2 44.7 8 32 8Z"
          />
          <ellipse cx="32" cy="13.5" rx="12.5" ry="5.2" fill="white" fillOpacity=".28" />
          <ellipse cx="32" cy="13.2" rx="8.8" ry="3.3" fill="white" fillOpacity=".82" />
        </svg>
      );
    case "AudioAccessory1,1": // HomePod
      return (
        <svg viewBox="0 0 64 64" fill="none" className={cls} aria-hidden="true">
          <path
            fill="currentColor"
            d="M18 13.5C18 7.7 24.2 5 32 5s14 2.7 14 8.5v35C46 55 40.2 59 32 59s-14-4-14-10.5v-35Z"
          />
          <ellipse cx="32" cy="12.5" rx="10.5" ry="3.8" fill="white" fillOpacity=".28" />
          <ellipse cx="32" cy="12.3" rx="7.2" ry="2.4" fill="white" fillOpacity=".8" />
        </svg>
      );
    case "AppleTV14,1": // Apple TV badge
      return (
        <svg viewBox="0 0 365 364.94" className={cls} aria-hidden="true">
          <path fill="currentColor" d={APPLE_TV_PATH} />
        </svg>
      );
    default: // generic speaker with sound waves
      return (
        <svg viewBox="0 0 64 64" fill="none" className={cls} aria-hidden="true">
          <path
            fill="currentColor"
            d="M10 27h10l12-10c2-1.6 5-.2 5 2.4v25.2c0 2.6-3 4-5 2.4L20 37H10a4 4 0 0 1-4-4v-2a4 4 0 0 1 4-4Z"
          />
          <path
            d="M43 23c4.7 4.8 4.7 13.2 0 18M49 17c8.1 8.2 8.1 21.8 0 30"
            stroke="currentColor"
            strokeWidth="4"
            strokeLinecap="round"
          />
        </svg>
      );
  }
}

function AirplayIconSection({ settings, onSaved }: SectionProps) {
  const { status, save } = useSave(onSaved);
  const preset = AIRPLAY_MODELS.some((m) => m.value === settings.airplay_model);

  return (
    <Section title="AirPlay device icon">
      <p className="mb-2 text-sm text-dim">
        Phones choose the icon in their AirPlay list from the advertised
        model. Senders may need to rediscover the speaker (toggle Wi-Fi or
        wait a minute) after changing this.
      </p>
      <div className="grid grid-cols-3 gap-2 sm:grid-cols-6">
        {AIRPLAY_MODELS.map((m) => {
          const selected = settings.airplay_model === m.value;
          return (
            <button
              key={m.label}
              className={`flex flex-col items-center gap-1.5 rounded-lg border p-3 text-[12px] ${
                selected
                  ? "border-accent bg-accent/10 text-fg"
                  : "border-line text-dim hover:text-fg"
              }`}
              onClick={() => save({ airplay_model: m.value })}
            >
              <AirplayModelIcon model={m.value} />
              <span>{m.label}</span>
            </button>
          );
        })}
      </div>
      {!preset && (
        <p className="mt-2 text-[13px] text-dim">
          Custom model: <span className="font-mono">{settings.airplay_model}</span>
        </p>
      )}
      <div className="mt-3 flex items-center justify-between gap-3 border-t border-line pt-2.5 pb-1.5">
        <div className="min-w-0">
          <div>Classic AirPlay only</div>
          <div className="text-[12px] text-dim">
            The speaker's play/pause/next buttons only work over classic
            AirPlay: iOS drops the classic control channel on AirPlay 2
            sessions, and AirPlay 2's own one is encrypted and not yet
            reverse-engineered. Trade: no multi-speaker audio while
            enabled.
          </div>
        </div>
        <Toggle
          checked={settings.airplay_classic}
          onChange={(v) => save({ airplay_classic: v })}
        />
      </div>
      <StatusText status={status} />
    </Section>
  );
}

function HomeAssistantSection({ settings, onSaved }: SectionProps) {
  const { status, save } = useSave(onSaved);
  const [broker, setBroker] = useState(settings.mqtt_broker);
  const [username, setUsername] = useState(settings.mqtt_username);
  const [password, setPassword] = useState(settings.mqtt_password);
  const dirty =
    broker !== settings.mqtt_broker ||
    username !== settings.mqtt_username ||
    password !== settings.mqtt_password;
  return (
    <Section title="Home Assistant">
      <p className="mb-2 text-sm text-dim">
        Point the speaker at your MQTT broker and it appears in Home
        Assistant automatically (MQTT discovery): playback, volume,
        battery graphs, pairing, and OS updates - installable straight
        from HA's update dashboard. Leave the broker empty to disable.
      </p>
      <div className="flex flex-col gap-2">
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm">Broker</span>
          <input
            className="w-56 rounded-lg border border-line bg-panel px-2 py-1.5 text-sm"
            placeholder="e.g. 192.168.1.89:1883"
            value={broker}
            onChange={(e) => setBroker(e.target.value)}
          />
        </label>
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm">Username</span>
          <input
            className="w-56 rounded-lg border border-line bg-panel px-2 py-1.5 text-sm"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </label>
        <label className="flex items-center justify-between gap-3">
          <span className="text-sm">Password</span>
          <input
            type="password"
            className="w-56 rounded-lg border border-line bg-panel px-2 py-1.5 text-sm"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <div className="flex items-center justify-end gap-2">
          <StatusText status={status} />
          <button
            className="rounded-lg bg-accent px-3 py-1.5 text-sm font-semibold text-accent-ink disabled:opacity-40"
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
          </button>
        </div>
      </div>
    </Section>
  );
}

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

function HardwarePage() {
  return (
    <div className="flex justify-center px-4 pt-6 pb-16">
      <main className="w-full max-w-lg">
        <p className="mt-2 text-[13px]">
          <a className="text-dim underline hover:text-fg" href="#">
            &larr; Back to settings
          </a>
        </p>
        <h1 className="mt-3 text-[22px] font-semibold">Box hardware</h1>
        <div className="mt-3 mb-4 rounded-lg border border-err/40 bg-err/10 p-3 text-[13px]">
          These settings describe this box&apos;s physical build and are
          written into the boot configuration. A wrong display overlay can
          leave the screen dark (the box stays reachable over ssh and this
          page); a wrong GPIO line can conflict with wiring. Only change
          them if you know the hardware.
        </div>
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
      <Section title="Box hardware">
        <p className="text-sm text-dim">
          Hardware configuration is <span className="text-fg">locked</span> on
          this box: the page and its API are disabled so nothing on the
          network can change the boot configuration. Administer it over ssh
          instead - <code>boompi-box</code> covers editing, applying,
          exporting a provisioning bundle, and <code>boompi-box unlock</code>{" "}
          to re-enable this page.
        </p>
      </Section>
    );
  }
  if (!profile) return null;
  const set = (patch: Partial<BoxProfile>) =>
    setProfile((p) => (p && p !== "locked" ? { ...p, ...patch } : p));

  const apply = async () => {
    if (
      !window.confirm(
        "Apply this hardware profile? It is written into the boot " +
          "configuration of both OS slots and takes effect on reboot.",
      )
    ) {
      return;
    }
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
    if (
      !window.confirm(
        "Lock hardware configuration? This page and its API turn off; " +
          "further changes require ssh (boompi-box). Unlock with " +
          "'boompi-box unlock'.",
      )
    ) {
      return;
    }
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

  const area =
    "w-full rounded-lg border border-line bg-bg px-3 py-2 font-mono text-[12px] focus:border-accent focus:outline-none";
  return (
    <Section title="Box hardware">
      <p className="mb-3 text-[13px] text-dim">
        This box&apos;s hardware profile (display, wiring, battery). Applied
        live to <code>/data/box/</code> and merged into the boot config; it
        survives OS updates. Download it as a bundle to provision another
        SD card (drop the extracted <code>boompi-box/</code> folder onto a
        freshly flashed card&apos;s boot partition).
      </p>
      <label className="mb-2 block text-[13px] text-dim">
        Preset
        <select
          className="mt-1 block w-full rounded-lg border border-line bg-bg px-2 py-2 text-sm"
          value=""
          onChange={(e) => {
            const p = BOX_PRESETS[e.target.value];
            if (p) {
              setProfile({ ...p, authorized_keys: profile.authorized_keys });
              setRebootNeeded(false);
            }
          }}
        >
          <option value="">Load a preset…</option>
          {Object.keys(BOX_PRESETS).map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
      </label>
      <label className="mb-2 block text-[13px] text-dim">
        config.txt fragment (dtoverlays, dtparams, GPIO)
        <textarea
          className={`${area} mt-1 h-28`}
          value={profile.config_txt ?? ""}
          onChange={(e) => set({ config_txt: e.target.value || null })}
        />
      </label>
      <label className="mb-2 block text-[13px] text-dim">
        Kernel arguments (single line; e.g. video= for an EDID-less panel)
        <input
          className={`${area} mt-1`}
          value={profile.cmdline_txt ?? ""}
          onChange={(e) => set({ cmdline_txt: e.target.value || null })}
        />
      </label>
      <label className="mb-2 block text-[13px] text-dim">
        hardware.toml (battery wiring/thresholds; [settings] seeds first boot)
        <textarea
          className={`${area} mt-1 h-20`}
          value={profile.hardware_toml ?? ""}
          onChange={(e) => set({ hardware_toml: e.target.value || null })}
        />
      </label>
      <label className="mb-2 block text-[13px] text-dim">
        Panel environment (e.g. SLINT_KMS_ROTATION=270)
        <textarea
          className={`${area} mt-1 h-12`}
          value={profile.env ?? ""}
          onChange={(e) => set({ env: e.target.value || null })}
        />
      </label>
      <label className="mb-3 block text-[13px] text-dim">
        SSH authorized keys (public keys, one per line - required before
        locking; ssh is key-only)
        <textarea
          className={`${area} mt-1 h-16`}
          placeholder="ssh-ed25519 AAAA... you@laptop"
          value={profile.authorized_keys ?? ""}
          onChange={(e) => set({ authorized_keys: e.target.value || null })}
        />
      </label>
      <div className="flex items-center gap-3">
        <button
          className="rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-accent-ink disabled:opacity-40"
          disabled={status.kind === "saving"}
          onClick={apply}
        >
          Apply to this box
        </button>
        <button
          className="rounded-lg border border-line px-4 py-2 text-sm text-dim"
          onClick={download}
        >
          Download bundle
        </button>
        <button
          className="rounded-lg border border-err/40 px-4 py-2 text-sm text-err hover:bg-err/10"
          onClick={lock}
          title="Requires an ssh key; unlock via ssh"
        >
          Lock
        </button>
        <StatusText status={status} />
      </div>
      {rebootNeeded && (
        <div className="mt-3 flex items-center gap-3 rounded-lg border border-line bg-bg p-3">
          <span className="text-[13px] text-dim">
            Boot config changed - reboot to apply.
          </span>
          <button
            className="rounded-lg border border-err/40 px-3 py-1 text-[13px] text-err hover:bg-err/10"
            onClick={() => sendCommand({ type: "reboot" })}
          >
            Reboot now
          </button>
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
