---
"boompi": patch
---

The iOS app now finds and controls speakers over Wi-Fi as well as
Bluetooth. It browses the network for the `_boompi._tcp` advert and
connects over the WebSocket protocol - same remote, faster pipe -
while BLE remains for boxes with no shared network. Wi-Fi boxes show
a wifi glyph in the speaker list where BLE rows show signal bars,
the remembered box auto-connects on whichever transport it was last
used, and a lost Wi-Fi link retries while the box stays advertised
(so boompid restarts and OTA reboots reconnect on their own).
