---
"boompi": minor
---

The settings web app grew out of its single endless scroll. Fourteen
stacked cards became nine focused pages behind a collapsible sidebar
(shadcn/ui) - General, Audio & AirPlay, Display, Bluetooth, Wi-Fi,
Games, Battery, Home Assistant, Software - so finding a setting is a
click, not an archaeology dig. Under the hood the whole UI moved to
shadcn components on a shared workspace package (@boompi/ui) that
also carries the protocol types and a transport abstraction: the same
section components now run against WebSocket+REST on the box or a
BLE GATT link on the hosted remote app, with IP-only features
(network scans, ROM uploads) degrading gracefully when there's no IP
path. Same dark boompi palette, real design system underneath.
