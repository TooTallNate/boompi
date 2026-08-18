# Boompi iOS app

Controls a Boompi speaker over BLE GATT (the same JSON protocol as the
WebSocket and the hosted remote at boompi.n8.io; contract in
`docs/BLE.md`). Discovery scans for the boompi control service, the
most recently used speaker auto-connects when seen, and every UI
section is capability-gated on `Hello.capabilities` so the app shows
only what the connected box's software actually supports.

## Layout

- `BoompiKit/` - Swift package with **all** the logic and UI:
  - `Chunking.swift` - BLE chunk framing (mirror of
    `boompi-proto::ble`, like the web's `ble.ts`)
  - `Proto.swift` - protocol models + capabilities
  - `BoompiClient.swift` - CoreBluetooth central: discovery,
    auto-(re)connect, chunked transport
  - `Views/` - SwiftUI (discovery + remote)
- `Boompi/` - the thin `@main` app shell
- `project.yml` - XcodeGen spec for the app target

## Developing

The package builds and tests with the plain Swift toolchain (no Xcode
required - CI and CLT-only machines can validate everything except the
app shell):

    cd BoompiKit && swift build && swift run BoompiKitChecks

(A plain executable stands in for a test target: XCTest and
swift-testing only ship with Xcode.)

## Building the app

Requires Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen):

    brew install xcodegen
    cd ios
    echo "settings: {}" > local.yml   # or with your DEVELOPMENT_TEAM (see below)
    xcodegen
    open Boompi.xcodeproj

`local.yml` is gitignored per-developer state; putting your team there
means regenerating the project never loses signing:

    settings:
      DEVELOPMENT_TEAM: ABCDE12345

Bluetooth needs real hardware - the iOS Simulator has no CoreBluetooth
radio. Build to a device.
