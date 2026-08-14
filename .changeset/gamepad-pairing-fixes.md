---
"boompi": patch
---

Gamepad pairing actually works now. Three bugs conspired against the
DualSense: bluetoothd shipped without its HID input profile (the pad
would pair, find nothing to connect to, and power itself off - now
enabled, with HoG for BLE pads), boompid's post-pair audio dial-back
treated the pad like a silent phone and disconnected it after 8
seconds (gamepads are now exempt), and the autopair flow flashed a
Pair/Reject dialog that auto-resolved before anyone could read it
(replaced by a proper "Pairing..." progress state on both the panel
and the web page).
