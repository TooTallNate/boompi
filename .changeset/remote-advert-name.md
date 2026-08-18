---
"boompi": patch
---

The control channel introduces itself properly: the BLE advert is now
named "Boompi Remote - <speaker name>", so a phone's Bluetooth list
shows two tellable-apart entries - the speaker (audio) and its remote
(control), the same pattern car keys use. The Boompi apps strip the
prefix and show just the speaker name. The advert name is trimmed to
BLE's 29-byte limit - BlueZ rejects oversized registrations outright
rather than truncating, which silently stopped one box advertising
until diagnosed in the field.
