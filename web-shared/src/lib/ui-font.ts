// The system UI font (Settings.font) in the web apps: the same family
// the speaker's panel uses, applied to the whole settings UI.
//
// - Built-ins (Geist + the five Geist Pixel styles) are the panel's own
//   vendored TTFs, bundled by Vite (fetched lazily - only the family in
//   use downloads).
// - Uploaded / downloaded families are served by the speaker at
//   /fonts/ui/... - used when the app has an IP path to it (box app).
// - Anything else is a Google Fonts family, loaded from Google's CSS
//   API (also the hosted remote's path for downloaded families).

import { useEffect } from "react";
import { BUILTIN_FONTS, DEFAULT_FONT, type FontState } from "@boompi/ui/proto";
import geist from "../../../rust/boompi-ui/ui/fonts/Geist.ttf?url";
import pixelSquare from "../../../rust/boompi-ui/ui/fonts/GeistPixel-Square.ttf?url";
import pixelGrid from "../../../rust/boompi-ui/ui/fonts/GeistPixel-Grid.ttf?url";
import pixelCircle from "../../../rust/boompi-ui/ui/fonts/GeistPixel-Circle.ttf?url";
import pixelTriangle from "../../../rust/boompi-ui/ui/fonts/GeistPixel-Triangle.ttf?url";
import pixelLine from "../../../rust/boompi-ui/ui/fonts/GeistPixel-Line.ttf?url";

const BUILTIN_URLS: Record<(typeof BUILTIN_FONTS)[number], string> = {
  Geist: geist,
  "Geist Pixel Square": pixelSquare,
  "Geist Pixel Grid": pixelGrid,
  "Geist Pixel Circle": pixelCircle,
  "Geist Pixel Triangle": pixelTriangle,
  "Geist Pixel Line": pixelLine,
};

export function isBuiltinFont(family: string): boolean {
  return (BUILTIN_FONTS as readonly string[]).includes(family);
}

const registered = new Set<string>();

function addFace(family: string, url: string, weight: string) {
  const key = `${family}|${url}`;
  if (registered.has(key)) return;
  registered.add(key);
  try {
    document.fonts.add(new FontFace(family, `url("${url}")`, { weight }));
  } catch {
    registered.delete(key);
  }
}

/** Make the built-ins available by name (lazy - nothing downloads
 *  until text uses a family). Geist is variable: one file, all weights. */
export function registerBuiltinFonts() {
  for (const [family, url] of Object.entries(BUILTIN_URLS)) {
    addFace(family, url, family === "Geist" ? "100 900" : "400");
  }
}

/** Weights learned from search results, so the Google CSS request
 *  asks only for weights the family has (css2 rejects others). */
const knownWeights = new Map<string, number[]>();
export function rememberWeights(family: string, weights: number[]) {
  knownWeights.set(family, weights);
}

/** Google Fonts css2 URL for a family (panel weights when known). */
export function googleCssUrl(family: string): string {
  const fam = encodeURIComponent(family).replace(/%20/g, "+");
  const have = knownWeights.get(family);
  const want = [300, 400, 600, 700].filter((w) => have?.includes(w));
  const axis = want.length > 1 ? `:wght@${want.join(";")}` : "";
  return `https://fonts.googleapis.com/css2?family=${fam}${axis}&display=swap`;
}

/** Load a family by name: built-in, served by the speaker, or Google. */
export function loadFont(family: string, font: FontState | undefined, sameOrigin: boolean) {
  if (isBuiltinFont(family)) {
    registerBuiltinFonts();
    return;
  }
  const installed = sameOrigin ? font?.installed.find((f) => f.family === family) : undefined;
  if (installed) {
    for (const f of installed.files) addFace(family, f.url, String(f.weight));
    return;
  }
  const href = googleCssUrl(family);
  const id = `gf-${family}`;
  if (document.getElementById(id)) return;
  const link = document.createElement("link");
  link.id = id;
  link.rel = "stylesheet";
  link.href = href;
  document.head.appendChild(link);
}

/** CSS font-family value with fallbacks. */
export function fontStack(family: string): string {
  return `"${family.replace(/"/g, "")}", ui-sans-serif, system-ui, sans-serif`;
}

/** Apply the speaker's font to the whole page. `sameOrigin` = the app
 *  is served by the speaker (it can load /fonts/ui/... files). */
export function useUiFont(family: string | undefined, font: FontState | undefined, sameOrigin: boolean) {
  const f = family || DEFAULT_FONT;
  // A failed download falls back to Geist, like the panel.
  const effective = font?.family === f && font.status === "error" ? DEFAULT_FONT : f;
  useEffect(() => {
    loadFont(effective, font, sameOrigin);
    const stack = fontStack(effective);
    document.documentElement.style.setProperty("--font-sans", stack);
    document.documentElement.style.fontFamily = stack;
  }, [effective, font, sameOrigin]);
}
