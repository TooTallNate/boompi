# BLE GATT control bridge

The Boompi protocol (the same JSON `ServerMessage` / `ClientMessage`
envelopes the WebSocket carries - see `rust/boompi-proto/src/lib.rs`)
is also served over a custom Bluetooth LE GATT service. This exists for
one reason: **controlling the speaker when the phone and the speaker
share no IP network** - camping, the beach, a parking lot. A phone that
is already streaming A2DP audio can control everything over the same
radio, with no Wi-Fi juggling.

Clients:

- **Native iOS/Android apps** (CoreBluetooth / android.bluetooth.le) -
  the primary audience. iOS cannot join a Bluetooth PAN and has no Web
  Bluetooth, so a GATT service is the only no-IP control path that
  reaches iPhones.
- **Web Bluetooth** (Chrome on Android/desktop) - works against this
  service, but requires the page to be served from an HTTPS origin, so
  the on-box web UI (plain HTTP) cannot use it. A separately hosted
  PWA could.

The Wi-Fi hotspot (Settings → Hotspot, or `wifi` protocol actions)
remains the full-fidelity fallback: joining the speaker's own AP gives
a phone the complete WebSocket experience (artwork, visualizer, OTA,
games) at Wi-Fi speeds.

## Service layout

Implemented in `rust/boompid/src/ble_gatt.rs`; UUIDs and framing
helpers live in `boompi_proto::ble` (port these constants verbatim to
Swift/Kotlin/TS).

| Item | UUID | Properties |
| --- | --- | --- |
| Primary service | `a5e90001-9c60-4b2a-a6ca-0d0a2b5f0e1f` | advertised |
| `control` characteristic | `a5e90002-9c60-4b2a-a6ca-0d0a2b5f0e1f` | write, write-without-response |
| `events` characteristic | `a5e90003-9c60-4b2a-a6ca-0d0a2b5f0e1f` | notify |
| `state` characteristic | `a5e90004-9c60-4b2a-a6ca-0d0a2b5f0e1f` | read |

The LE advertisement carries the service UUID and the speaker name as
`LocalName`, so apps can scan-filter on the UUID and label results
without connecting.

## Chunk framing

JSON messages routinely exceed the ATT MTU, so `control` writes and
`events` notifications carry *chunked* messages. Each chunk is:

```
byte 0:   flags     bit 0 (0x01) = FIRST chunk of a message
                    bit 1 (0x02) = LAST  chunk of a message
byte 1..: payload   raw JSON bytes
```

- A message that fits one chunk has `flags = FIRST|LAST = 0x03`.
- Reassembly: on FIRST, reset the buffer; append payload; on LAST, the
  buffer is one complete JSON message.
- A continuation chunk with no preceding FIRST is dropped (client and
  server both resynchronize on the next FIRST).
- Messages are capped at 64 KiB (`ble::MAX_MESSAGE`); larger is a
  framing error.
- Chunk sizing: boompid uses the ATT MTU when BlueZ reports one in
  read/write options (notification payload = MTU − 3), else a
  conservative 176 bytes (fits iOS's default 185-byte MTU). Clients
  should negotiate the largest MTU available and chunk their writes to
  `MTU − 3`.

## Session flow

1. Scan for the service UUID, connect, discover characteristics.
2. Subscribe to `events`. boompid greets every new subscription with
   `hello` followed by a full `state` snapshot (same as a WebSocket
   connect), then streams the regular deltas (`track`, `volume`,
   `battery`, `settings`, `wifi`, ...).
3. Alternatively (or additionally), long-read `state` for a snapshot:
   an offset-0 read regenerates it; offset continuations slice that
   same snapshot so multi-request reads never tear.
4. Send commands as chunked JSON `ClientMessage` writes to `control`
   (`{"type":"set_volume","level":0.5}`, `{"type":"next"}`,
   `{"type":"wifi","action":"ap","enabled":true}`, ...).

A neat trick for a native app: send the hotspot-on command over BLE,
join the speaker's AP with `NEHotspotConfigurationManager`, and switch
to the WebSocket for the heavy assets - BLE is the always-works
bootstrap, IP is the fast path.

## Not carried over BLE

- **Binary frames** (visualizer bars at ~30 fps, artwork bytes): they
  would swamp a ~5-50 KB/s LE link. Fetch artwork via `GET /art/{id}`
  when an IP path exists; skip the visualizer.
- **`battery_fast_poll`**: it is refcounted per WebSocket connection
  and released on disconnect; BLE has no equivalent lifecycle hook, so
  boompid ignores it on this transport. Battery deltas still arrive at
  the default cadence.

## Security model

Identical to the LAN HTTP/WebSocket API and the open hotspot: the
control channel is unauthenticated (LE JustWorks, no bond required).
The speaker's A2DP pairing window (`bt_agent.rs`) is unrelated and
unaffected - GATT clients neither need nor trigger classic pairing.
If a stronger model is ever wanted, requiring an encrypted+bonded link
is a one-line flags change (`encrypt-authenticated-write` etc.), at the
cost of a pairing prompt on first app connect.

## Operational notes

- Requires a BlueZ adapter in (default) `dual` ControllerMode; the
  GATT app registers against whichever adapter exposes
  `org.bluez.GattManager1`. bluetoothd restarts are detected and the
  registration re-established automatically.
- LE advertising registration is best-effort: some dongles (CSR8510)
  have flaky LE support. The GATT service still functions for clients
  that know the address; test advertising on such boxes before relying
  on scan-discovery.
- Speaker renames re-register the advertisement under the new
  `LocalName`.
