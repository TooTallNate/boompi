import { Badge } from "@boompi/ui/components/badge";
import { Button } from "@boompi/ui/components/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Separator } from "@boompi/ui/components/separator";
import { Spinner } from "@boompi/ui/components/spinner";
import { capsOf } from "@boompi/ui/proto";
import { useBoompi } from "@boompi/ui/transport";
import { Fragment } from "react";

/** Fully push-driven: the ws State snapshot carries the catalog and
 *  every change (downloads incl. progress, selection) arrives as an
 *  emoji_fonts broadcast - no REST polling. */
export function EmojiFontsSection() {
  const { state, send, hello } = useBoompi();
  const emoji = state?.emoji_fonts;
  if (!emoji || !capsOf(hello).has("emoji_fonts")) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Emoji style</CardTitle>
        <CardDescription>
          The font used for emoji on the speaker's screen (name, track
          titles). Downloads are stored on the speaker and survive updates;
          switching restarts the panel UI briefly.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {emoji.fonts.map((f, i) => (
          <Fragment key={f.id}>
            {i > 0 && <Separator />}
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <span>{f.label}</span>
                  {f.active && <Badge variant="secondary">active</Badge>}
                </div>
                <div className="text-xs text-muted-foreground">
                  {f.license}
                  {f.size > 0 && !f.installed && (
                    <span className="ml-2">{Math.round(f.size / 1024 / 1024)} MB</span>
                  )}
                </div>
              </div>
              <div className="flex flex-none gap-2">
                {f.installed && !f.active && (
                  <Button
                    size="sm"
                    onClick={() => send({ type: "emoji_font", action: "select", id: f.id })}
                  >
                    Use
                  </Button>
                )}
                {!f.installed &&
                  (emoji.downloading === f.id ? (
                    <span className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Spinner />
                      {Math.round((emoji.progress ?? 0) * 100)}%
                    </span>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={emoji.downloading != null}
                      onClick={() => send({ type: "emoji_font", action: "download", id: f.id })}
                    >
                      Download
                    </Button>
                  ))}
                {f.installed && !f.builtin && !f.active && (
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => send({ type: "emoji_font", action: "remove", id: f.id })}
                  >
                    Remove
                  </Button>
                )}
              </div>
            </div>
          </Fragment>
        ))}
        {emoji.error && <p className="text-xs text-destructive">{emoji.error}</p>}
      </CardContent>
    </Card>
  );
}
