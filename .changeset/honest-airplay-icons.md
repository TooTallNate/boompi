---
"boompi": patch
---

Fix AirPlay connections failing on current iOS with the Bookshelf or
TV icon presets: the third-party icon feature bits double as
authentication requirements (bit 26 demands MFi hardware auth and the
sender aborts the handshake; bit 51 demands HomeKit PIN pairing). The
non-Apple icon presets are removed; Generic and the Apple model
presets (HomePod mini, HomePod, Apple TV) connect fine. Boxes that
had the Bookshelf or TV preset selected keep their custom model name
but no longer advertise the poisoned bits.
