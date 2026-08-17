import { useRef, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Slider } from "@boompi/ui/components/slider";
import { useBoompi } from "@boompi/ui/transport";

/** Master volume: same ClientMessage the panel slider sends
 *  (set_volume -> PipeWire sink + AVRCP echo to the phone). Local
 *  state while dragging so the server's volume broadcasts (from the
 *  panel or the phone) don't fight the thumb mid-gesture. */
export function VolumeSection() {
  const { state, send } = useBoompi();
  const volume = state?.volume ?? 0;
  const [dragging, setDragging] = useState(false);
  const [local, setLocal] = useState(volume);
  const shown = dragging ? local : volume;
  const throttle = useRef<number>(0);
  const trailing = useRef<ReturnType<typeof setTimeout> | null>(null);

  const push = (level: number) => {
    // Leading-edge immediate, then at most ~10/s with a trailing
    // flush - mirrors the panel slider's throttle.
    const now = Date.now();
    if (now - throttle.current >= 100) {
      throttle.current = now;
      send({ type: "set_volume", level });
    } else {
      if (trailing.current) clearTimeout(trailing.current);
      trailing.current = setTimeout(() => {
        throttle.current = Date.now();
        send({ type: "set_volume", level });
      }, 100);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Volume</CardTitle>
        <CardDescription>Speaker volume: {Math.round(shown * 100)}%</CardDescription>
      </CardHeader>
      <CardContent>
        <Slider
          min={0}
          max={100}
          value={[Math.round(shown * 100)]}
          onValueChange={([v]) => {
            setDragging(true);
            const level = v / 100;
            setLocal(level);
            push(level);
          }}
          onValueCommit={() => setDragging(false)}
        />
      </CardContent>
    </Card>
  );
}
