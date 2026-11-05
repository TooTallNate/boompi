// Shared formatting for an in-flight OS update, used by the Software
// page and the sidebar/header indicators so they never disagree.

import type { UpdateState } from "@boompi/ui/proto";

export const STAGE_LABEL: Record<string, string> = {
  downloading_system: "downloading system",
  verifying_system: "verifying system",
  downloading_boot: "downloading boot files",
  verifying_boot: "verifying boot files",
  restarting: "restarting",
};

const UNITS = ["KB", "MB", "GB", "TB"];

/** Decimal (SI) units: "512 B", "312.4 MB", "1.2 GB". */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 1000) return `${Math.max(0, Math.round(bytes || 0))} B`;
  let v = bytes;
  let unit = -1;
  // 999.96 KB would print as "1000.0 KB"; roll over on the rounded value.
  while (v >= 999.95 && unit < UNITS.length - 1) {
    v /= 1000;
    unit++;
  }
  return `${v.toFixed(1)} ${UNITS[unit]}`;
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatEta(secs: number): string {
  if (!Number.isFinite(secs) || secs < 60) return "less than a minute left";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `about ${mins} min left`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  return m > 0 ? `about ${h} hr ${m} min left` : `about ${h} hr left`;
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export interface UpdateDescription {
  /** An update is being applied (stays true through the reboot). */
  active: boolean;
  /** Version being installed. */
  version: string | null;
  /** 0-100, or null when the box hasn't reported progress yet. */
  percent: number | null;
  /** "Downloading system…", "Restarting into v2.3.0…". */
  stageLabel: string;
  restarting: boolean;
  /** "312.4 MB of 640.0 MB" (only while downloading, newer boxes). */
  bytesLine?: string;
  /** "2.3 MB/s" (only while bytes are flowing). */
  rateLine?: string;
  /** "about 2 min left", or a waiting/stalled message at 0 B/s. */
  etaLine?: string;
  /** Throughput is 0 mid-download (reconnecting). */
  stalled: boolean;
}

export function describeUpdate(updates: UpdateState | null | undefined): UpdateDescription {
  const version = updates?.applying ?? null;
  if (!updates || version == null) {
    return { active: false, version: null, percent: null, stageLabel: "", restarting: false, stalled: false };
  }

  const stage = updates.stage;
  const restarting = stage === "restarting";
  const downloading = stage === "downloading_system" || stage === "downloading_boot";
  const percent =
    updates.progress == null ? null : Math.min(100, Math.max(0, Math.round(updates.progress * 100)));

  const stageLabel = restarting
    ? `Restarting into ${version}…`
    : `${capitalize(STAGE_LABEL[stage ?? ""] ?? "preparing")}…`;

  const desc: UpdateDescription = { active: true, version, percent, stageLabel, restarting, stalled: false };
  if (!downloading) return desc;

  const done = updates.downloaded_bytes;
  const total = updates.total_bytes;
  const rate = updates.bytes_per_second;

  if (done != null && total != null && total > 0) {
    desc.bytesLine = `${formatBytes(done)} of ${formatBytes(total)}`;
  }
  if (rate != null) {
    if (rate > 0) {
      desc.rateLine = formatRate(rate);
      if (done != null && total != null && total > done) {
        desc.etaLine = formatEta((total - done) / rate);
      }
    } else {
      desc.stalled = true;
      desc.etaLine = done ? "Waiting for connection…" : "Connecting…";
    }
  }
  return desc;
}
