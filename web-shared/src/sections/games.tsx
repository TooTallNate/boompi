import { useState } from "react";
import { Alert, AlertDescription } from "@boompi/ui/components/alert";
import { Button } from "@boompi/ui/components/button";
import { ConfirmButton } from "@boompi/ui/components/confirm-button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldLabel } from "@boompi/ui/components/field";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@boompi/ui/components/select";
import { Slider } from "@boompi/ui/components/slider";
import { StatusText } from "@boompi/ui/components/status-text";
import { useBoompi, useSave } from "@boompi/ui/transport";

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

export function GamesSection() {
  const { state, send, rest } = useBoompi();
  const [system, setSystem] = useState<string>("nes");
  // Radix sliders are frozen when controlled without onValueChange:
  // hold the thumb in local state while dragging, commit on release.
  const [volDrag, setVolDrag] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { save, status } = useSave();
  const games = state?.games;
  const settings = state?.settings;
  if (!games || !settings) return null;

  const upload = async (files: FileList | null) => {
    if (!rest || !files || files.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      await rest.uploadGames(system, files);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const del = async (system: string, file: string) => {
    if (!rest) return;
    await rest.deleteGame(system, file);
  };

  const freeGB = (games.storage_free / 1e9).toFixed(1);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Games</CardTitle>
        <CardDescription>
          RetroArch is aboard.{" "}
          {rest ? (
            <>
              Upload your ROMs here or drag them onto the network share (
              <code>smb://{rest.host}/games</code>, guest access),
            </>
          ) : (
            <>Upload ROMs from the Wi-Fi settings page,</>
          )}{" "}
          pair a controller (same pairing button as speakers), launch from the
          panel. Music and gameplay mix on separate tracks, each with its own
          volume.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {games.running && (
          <Alert>
            <AlertDescription>
              <div className="flex w-full items-center justify-between gap-3">
                <span>
                  Playing: <code>{games.running}</code>
                </span>
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={() => send({ type: "game", action: "stop" })}
                >
                  Stop game
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}
        {rest && (
          <div className="flex items-center gap-2">
            <Select value={system} onValueChange={setSystem}>
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {GAME_SYSTEMS.map(([id, label]) => (
                    <SelectItem key={id} value={id}>
                      {label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <Button asChild disabled={busy}>
              <label className="cursor-pointer">
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
            </Button>
            <span className="text-xs text-muted-foreground">{freeGB}GB free</span>
          </div>
        )}
        {error && <p className="text-xs text-destructive">{error}</p>}
        {games.games.length > 0 && (
          <ul>
            {games.games.map((g) => (
              <li
                key={`${g.system}/${g.file}`}
                className="flex items-center gap-3 border-t py-1.5 first:border-t-0"
              >
                <span className="w-10 text-[11px] text-primary">{g.system}</span>
                <span className="min-w-0 flex-1 truncate text-sm">{g.name}</span>
                <span className="text-[11px] text-muted-foreground">
                  {(g.size / 1e6).toFixed(1)}MB
                </span>
                {g.system !== "bios" && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      send({ type: "game", action: "launch", system: g.system, file: g.file })
                    }
                  >
                    Play
                  </Button>
                )}
                {rest && (
                  <ConfirmButton
                    variant="ghost"
                    size="sm"
                    className="text-muted-foreground hover:text-destructive"
                    title={`Delete ${g.file}?`}
                    description="Save files are kept."
                    confirmLabel="Delete"
                    confirmVariant="destructive"
                    onConfirm={() => void del(g.system, g.file)}
                  >
                    delete
                  </ConfirmButton>
                )}
              </li>
            ))}
          </ul>
        )}
        <Field>
          <FieldLabel>
            Game volume: {volDrag ?? Math.round(settings.game_volume * 100)}%
          </FieldLabel>
          <Slider
            min={0}
            max={100}
            value={[volDrag ?? Math.round(settings.game_volume * 100)]}
            onValueChange={([v]) => setVolDrag(v)}
            onValueCommit={([v]) => {
              save({ game_volume: v / 100 });
              setVolDrag(null);
            }}
          />
        </Field>
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
