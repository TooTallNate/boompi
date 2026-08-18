import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@boompi/ui/components/card";
import { Field, FieldDescription, FieldLabel } from "@boompi/ui/components/field";
import { Separator } from "@boompi/ui/components/separator";
import { Switch } from "@boompi/ui/components/switch";
import { ToggleGroup, ToggleGroupItem } from "@boompi/ui/components/toggle-group";
import { StatusText } from "@boompi/ui/components/status-text";
import { capsOf } from "@boompi/ui/proto";
import { useBoompi, useSave } from "@boompi/ui/transport";

// Values are mDNS `model=` strings. Senders resolve Apple model strings
// to product icons (only the HomePods and Apple TV have any). There are
// no non-Apple presets: the third-party icon feature bits are
// booby-trapped on current iOS - bit 26 (bookshelf icon) doubles as
// Authentication_4 and makes senders abort the handshake demanding MFi
// auth, bit 51 draws the icon but demands HomeKit PIN pairing we don't
// implement.
const AIRPLAY_MODELS: { label: string; value: string }[] = [
  { label: "Generic speaker", value: "" },
  { label: "HomePod mini", value: "AudioAccessory5,1" },
  { label: "HomePod", value: "AudioAccessory1,1" },
  { label: "Apple TV", value: "AppleTV14,1" },
];

// The Apple TV badge outline+wordmark, as a single silhouette path.
const APPLE_TV_PATH =
  "M 267.285156 232.710938 L 249.253906 232.710938 L 222.746094 158.136719 L 240.238281 158.136719 L 258.710938 215.410156 L 258.988281 215.410156 L 276.773438 158.136719 L 293.367188 158.136719 Z M 214.828125 170.355469 L 200.304688 170.355469 L 200.304688 210.152344 C 200.304688 211.882813 200.40625 213.410156 200.554688 214.644531 C 200.6875 215.910156 201.011719 216.964844 201.554688 217.820313 C 202.054688 218.699219 202.816406 219.316406 203.800781 219.757813 C 204.902344 220.257813 206.3125 220.433594 208.105469 220.433594 C 209.191406 220.433594 210.320313 220.402344 211.46875 220.34375 C 212.597656 220.316406 213.699219 220.167969 214.828125 219.875 L 214.828125 232.5625 C 213.066406 232.769531 211.335938 232.914063 209.601563 233.148438 C 207.957031 233.296875 206.210938 233.386719 204.433594 233.386719 C 200.203125 233.386719 196.796875 232.972656 194.183594 232.238281 C 191.613281 231.359375 189.597656 230.152344 188.117188 228.597656 C 186.679688 226.953125 185.691406 225.015625 185.164063 222.605469 C 184.679688 220.167969 184.402344 217.496094 184.238281 214.382813 L 184.238281 170.355469 L 172.109375 170.355469 L 172.109375 158.136719 L 184.238281 158.136719 L 184.238281 135.8125 L 200.304688 135.8125 L 200.304688 158.136719 L 214.828125 158.136719 Z M 147.054688 222.699219 C 142.621094 229.171875 138.054688 235.609375 130.8125 235.742188 C 123.707031 235.875 121.414063 231.519531 113.292969 231.519531 C 105.171875 231.519531 102.632813 235.609375 95.90625 235.875 C 88.929688 236.136719 83.613281 228.890625 79.148438 222.453125 C 70.058594 209.269531 63.066406 185.214844 72.4375 168.988281 C 77.089844 160.925781 85.375 155.8125 94.378906 155.675781 C 101.25 155.558594 107.699219 160.285156 111.914063 160.285156 C 116.039063 160.285156 123.46875 154.765625 132.179688 155.402344 C 135.613281 155.667969 145.335938 156.6875 151.605469 165.953125 C 151.109375 166.265625 140.050781 172.71875 140.167969 186.148438 C 140.300781 202.207031 154.234375 207.554688 154.398438 207.613281 C 154.265625 208 152.164063 215.234375 147.054688 222.699219 Z M 116.964844 135.164063 C 120.957031 130.476563 127.734375 126.980469 133.316406 126.757813 C 134.023438 133.25 131.421875 139.808594 127.546875 144.476563 C 123.652344 149.183594 117.300781 152.835938 111.074219 152.351563 C 110.199219 145.976563 113.359375 139.316406 116.964844 135.164063 Z M 364.867188 100.128906 C 364.867188 96.484375 364.867188 92.871094 364.851563 89.199219 C 364.589844 81.300781 364.089844 73.339844 362.632813 65.496094 C 361.296875 57.539063 358.933594 50.078125 355.277344 42.910156 C 351.636719 35.804688 346.9375 29.3125 341.28125 23.671875 C 335.6875 18.09375 329.164063 13.335938 322.085938 9.722656 C 314.816406 6.050781 307.429688 3.671875 299.46875 2.289063 C 291.703125 0.851563 283.714844 0.382813 275.769531 0.175781 C 272.140625 0.117188 268.441406 0.0898438 264.828125 0 L 100.097656 0 C 96.457031 0.0898438 92.871094 0.117188 89.183594 0.175781 C 81.300781 0.382813 73.328125 0.851563 65.410156 2.289063 C 57.492188 3.671875 50.105469 6.050781 42.882813 9.722656 C 35.761719 13.335938 29.296875 18.09375 23.6875 23.671875 C 18.019531 29.3125 13.320313 35.804688 9.734375 42.910156 C 6.007813 50.078125 3.6875 57.539063 2.292969 65.496094 C 0.867188 73.339844 0.367188 81.300781 0.191406 89.199219 C 0.0742188 92.871094 0.0585938 96.484375 0 100.128906 L 0 264.8125 C 0.0585938 268.484375 0.0742188 272.097656 0.191406 275.769531 C 0.367188 283.699219 0.867188 291.65625 2.292969 299.5 C 3.6875 307.460938 6.007813 314.890625 9.734375 322.117188 C 13.320313 329.164063 18.019531 335.6875 23.6875 341.238281 C 29.296875 346.90625 35.761719 351.632813 42.882813 355.21875 C 50.105469 358.917969 57.492188 361.269531 65.410156 362.679688 C 73.328125 364.089844 81.300781 364.558594 89.183594 364.792969 C 92.871094 364.882813 96.457031 364.910156 100.097656 364.910156 C 104.429688 364.941406 108.71875 364.941406 113.066406 364.941406 L 251.992188 364.941406 C 256.234375 364.941406 260.566406 364.941406 264.828125 364.910156 C 268.441406 364.910156 272.140625 364.882813 275.769531 364.792969 C 283.714844 364.558594 291.703125 364.089844 299.46875 362.679688 C 307.429688 361.269531 314.816406 358.917969 322.085938 355.21875 C 329.164063 351.632813 335.6875 346.90625 341.28125 341.238281 C 346.9375 335.6875 351.636719 329.164063 355.277344 322.117188 C 358.933594 314.890625 361.296875 307.460938 362.632813 299.5 C 364.089844 291.65625 364.589844 283.699219 364.851563 275.769531 C 364.867188 272.097656 364.867188 268.484375 364.867188 264.8125 C 365 260.523438 365 256.265625 365 251.859375 L 365 113.078125 C 365 108.734375 365 104.414063 364.867188 100.128906";

function AirplayModelIcon({ model }: { model: string }) {
  const cls = "size-9";
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

export function AirplaySection() {
  const { state, hello } = useBoompi();
  const { status, save } = useSave();
  const settings = state?.settings;
  if (!settings || !capsOf(hello).has("airplay")) return null;
  const preset = AIRPLAY_MODELS.some((m) => m.value === settings.airplay_model);

  return (
    <Card>
      <CardHeader>
        <CardTitle>AirPlay device icon</CardTitle>
        <CardDescription>
          Phones choose the icon in their AirPlay list from the advertised
          model. Senders may need to rediscover the speaker (toggle Wi-Fi or
          wait a minute) after changing this.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <ToggleGroup
          type="single"
          variant="outline"
          className="grid grid-cols-2 sm:grid-cols-4"
          value={settings.airplay_model}
          onValueChange={(m) => m !== undefined && save({ airplay_model: m })}
        >
          {AIRPLAY_MODELS.map((m) => (
            <ToggleGroupItem
              key={m.label}
              value={m.value}
              className="h-auto flex-col gap-1.5 p-3 text-xs"
            >
              <AirplayModelIcon model={m.value} />
              <span>{m.label}</span>
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {!preset && (
          <p className="text-xs text-muted-foreground">
            Custom model: <span className="font-mono">{settings.airplay_model}</span>
          </p>
        )}
        <Separator />
        <Field orientation="horizontal">
          <div className="flex flex-col gap-1">
            <FieldLabel htmlFor="airplay-classic">Classic AirPlay only</FieldLabel>
            <FieldDescription>
              The speaker's play/pause/next buttons only work over classic
              AirPlay: iOS drops the classic control channel on AirPlay 2
              sessions, and AirPlay 2's own one is encrypted and not yet
              reverse-engineered. Trade: no multi-speaker audio while enabled.
            </FieldDescription>
          </div>
          <Switch
            id="airplay-classic"
            checked={settings.airplay_classic}
            onCheckedChange={(v) => save({ airplay_classic: v })}
          />
        </Field>
        <StatusText status={status} />
      </CardContent>
    </Card>
  );
}
