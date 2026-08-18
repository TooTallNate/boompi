---
"boompi": patch
---

The native iOS app exists (ios/): CoreBluetooth against the same GATT
control bridge the hosted remote uses, so it works with no Wi-Fi, no
account, no setup. It scans for the boompi service, auto-connects to
the most recently used speaker the moment it's in range (the common
one-boompi case never sees a picker), offers the phone's clock to the
RTC-less box on connect, and gates every section on the box's
declared capabilities - a hard-wired box shows no battery, an
un-updated box explains its missing Wi-Fi management instead of
breaking. All logic lives in a Swift package that builds and
self-checks with the bare toolchain; Xcode is only needed to produce
the app itself. Nothing in the OS image changes - changelog trail
entry.
