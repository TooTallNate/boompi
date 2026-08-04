import type { Settings, SettingsPatch, StateResponse } from "./proto";

export async function fetchState(): Promise<StateResponse> {
  const r = await fetch("/api/state");
  if (!r.ok) throw new Error(`state fetch failed: HTTP ${r.status}`);
  return r.json();
}

export async function patchSettings(patch: SettingsPatch): Promise<Settings> {
  const r = await fetch("/api/settings", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(patch),
  });
  if (!r.ok) throw new Error(`settings update failed: HTTP ${r.status}`);
  return r.json();
}
