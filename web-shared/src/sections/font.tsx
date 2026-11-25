import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { Badge } from "@boompi/ui/components/badge";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Input } from "@boompi/ui/components/input";
import { Spinner } from "@boompi/ui/components/spinner";
import { StatusText } from "@boompi/ui/components/status-text";
import { BUILTIN_FONTS, DEFAULT_FONT, capsOf } from "@boompi/ui/proto";
import { fontStack, loadFont, rememberWeights } from "@boompi/ui/lib/ui-font";
import { cn } from "@boompi/ui/lib/utils";
import { useBoompi, useSave } from "@boompi/ui/transport";

const LABELS: Record<string, string> = {
  Geist: "Geist",
  "Geist Pixel Square": "Pixel Square",
  "Geist Pixel Grid": "Pixel Grid",
  "Geist Pixel Circle": "Pixel Circle",
  "Geist Pixel Triangle": "Pixel Triangle",
  "Geist Pixel Line": "Pixel Line",
};

function FontTile({
  family,
  label,
  active,
  onPick,
  onRemove,
  badge,
}: {
  family: string;
  label: string;
  active: boolean;
  onPick: () => void;
  onRemove?: () => void;
  badge?: string;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-1 rounded-md border px-3 py-2 text-sm transition-colors",
        active ? "border-primary bg-secondary" : "border-border hover:bg-muted",
      )}
    >
      <button type="button" className="min-w-0 truncate" style={{ fontFamily: fontStack(family) }} onClick={onPick}>
        {label}
      </button>
      {badge && (
        <Badge variant="outline" className="ml-1 text-[10px]">
          {badge}
        </Badge>
      )}
      {onRemove && (
        <button
          type="button"
          aria-label={`Remove ${family}`}
          className="ml-1 text-muted-foreground hover:text-destructive"
          onClick={onRemove}
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}

/** System UI font: built-ins, installed (uploaded / downloaded)
 *  families, Google Fonts search (over the protocol - works on BLE),
 *  and uploads (REST - box app only). */
export function FontSection() {
  const { state, send, hello, rest } = useBoompi();
  const { status, save } = useSave();
  const [query, setQuery] = useState("");
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const has = capsOf(hello).has("font");

  // Debounced search; an empty query lists the most popular families.
  useEffect(() => {
    if (!has) return;
    const t = setTimeout(() => send({ type: "font_search", query: query.trim() }), 300);
    return () => clearTimeout(t);
  }, [query, has, send]);

  // Installed tiles preview in their own face.
  const sameOrigin = rest != null;
  useEffect(() => {
    for (const f of state?.font?.installed ?? []) loadFont(f.family, state?.font, sameOrigin);
  }, [state?.font, sameOrigin]);

  const settings = state?.settings;
  const font = state?.font;
  if (!settings || !has) return null;
  const current = settings.font ?? DEFAULT_FONT;
  const results = state?.font_results?.query === query.trim() ? state.font_results : undefined;
  const installed = font?.installed ?? [];

  const pick = (family: string) => {
    if (family !== current) void save({ font: family });
  };

  const upload = async (files: FileList | null) => {
    if (!rest || !files || files.length === 0) return;
    setUploading(true);
    setUploadError(null);
    try {
      await rest.uploadFonts(files);
    } catch (e) {
      setUploadError(String(e instanceof Error ? e.message : e));
    } finally {
      setUploading(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Font</CardTitle>
        <CardDescription>
          One typeface for the whole speaker screen (screensaver included) and these settings. Pick
          a built-in, any Google Fonts family, or upload your own.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">Current:</span>
          <span style={{ fontFamily: fontStack(current) }}>{current}</span>
          {font?.family === current && font.status === "downloading" && (
            <span className="flex items-center gap-1 text-muted-foreground">
              <Spinner /> downloading to the speaker…
            </span>
          )}
          {font?.family === current && font.status === "error" && (
            <span className="text-destructive">{font.error ?? "download failed"} - showing Geist</span>
          )}
        </div>

        <div className="flex flex-wrap gap-2">
          {BUILTIN_FONTS.map((f) => (
            <FontTile key={f} family={f} label={LABELS[f] ?? f} active={current === f} onPick={() => pick(f)} />
          ))}
        </div>

        {installed.length > 0 && (
          <div className="flex flex-col gap-2">
            <span className="text-xs text-muted-foreground">On the speaker</span>
            <div className="flex flex-wrap gap-2">
              {installed.map((f) => (
                  <FontTile
                    key={f.family}
                    family={f.family}
                    label={f.family}
                    badge={f.source === "upload" ? "uploaded" : "Google"}
                    active={current === f.family}
                    onPick={() => pick(f.family)}
                    onRemove={() => send({ type: "font_remove", family: f.family })}
                  />
              ))}
            </div>
          </div>
        )}

        <div className="flex flex-col gap-2">
          <span className="text-xs text-muted-foreground">Google Fonts</span>
          <Input
            placeholder="Search 1,900+ families (e.g. Inter, Press Start 2P)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {results?.error && <p className="text-xs text-destructive">{results.error}</p>}
          {results && !results.error && (
            <ul className="max-h-64 overflow-y-auto rounded-md border">
              {results.fonts.length === 0 && (
                <li className="px-3 py-2 text-sm text-muted-foreground">No matching families</li>
              )}
              {results.fonts.map((f) => (
                <li key={f.family} className="border-t first:border-t-0">
                  <button
                    type="button"
                    className={cn(
                      "flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm hover:bg-muted",
                      current === f.family && "bg-secondary",
                    )}
                    onClick={() => {
                      rememberWeights(f.family, f.weights);
                      pick(f.family);
                    }}
                  >
                    <span className="truncate">{f.family}</span>
                    <span className="flex-none text-xs text-muted-foreground">
                      {f.category} · {f.weights.length} weight{f.weights.length === 1 ? "" : "s"}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {rest && (
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <Button asChild variant="outline" size="sm" disabled={uploading}>
                <label className="cursor-pointer">
                  {uploading ? "Uploading…" : "Upload font"}
                  <input
                    type="file"
                    multiple
                    accept=".ttf,.otf,font/ttf,font/otf"
                    className="hidden"
                    disabled={uploading}
                    onChange={(e) => {
                      void upload(e.target.files);
                      e.target.value = "";
                    }}
                  />
                </label>
              </Button>
              <span className="text-xs text-muted-foreground">.ttf or .otf, up to 30 MB each</span>
            </div>
            {uploadError && <p className="text-xs text-destructive">{uploadError}</p>}
          </div>
        )}
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
