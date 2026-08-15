---
"boompi": patch
---

Pairing a phone while a gamepad is connected actually works now. The
pairing window used to start an active gamepad scan, and on the USB
dongle that inquiry traffic starves the listening side of the radio -
the phone couldn't find or connect to the box until the gamepad was
disconnected. The scan now only runs when nothing is connected;
pairing a second gamepad just requires disconnecting the first.
