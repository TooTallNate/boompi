import type { SaveStatus } from "@boompi/ui/transport";

export function StatusText({ status }: { status: SaveStatus }) {
  switch (status.kind) {
    case "idle":
      return <span className="text-xs" />;
    case "saving":
      return <span className="text-xs text-muted-foreground">saving…</span>;
    case "ok":
      return <span className="text-xs text-success">saved</span>;
    case "err":
      return <span className="text-xs text-destructive">{status.message}</span>;
  }
}
