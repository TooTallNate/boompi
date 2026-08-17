---
"boompi": patch
---

There's now a hosted remote control at boompi-remote.vercel.app: the
same settings UI as the box's own web app (shared @boompi/ui
sections), but connected over Web Bluetooth to the speaker's BLE GATT
control bridge - no shared Wi-Fi, no IP network, no install. The
browser's Bluetooth chooser is the discovery step (it lists nearby
boompis by their advertised control service), and the link speaks the
identical JSON protocol as the WebSocket, chunk-framed to the ATT MTU.
IP-only features (network scans, ROM uploads, timezone) explain
themselves and point at the on-box settings page; the hotspot toggle
works over BLE as the escape hatch that creates an IP path. Chrome and
Edge today; iOS needs the upcoming native app (Safari has no Web
Bluetooth). Nothing in the OS image changes - this entry is for the
changelog trail.
