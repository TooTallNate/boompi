---
"boompi": patch
---

Bluetooth on the Pi 3 no longer corrupts itself under concurrent load.
The Pi 3's USB controller can complete transfers out of order when a
gamepad, a pairing burst, and USB audio all share the bus; the
Bluetooth dongle's HCI stream then reassembles garbage and the radio
wedges until a reboot (one crash of the Bluetooth daemon on the bench
traced back to this). The kernel's force_poll_sync option serializes
the completion path and is now set for every box.
