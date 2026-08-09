---
"boompi": patch
---

The panel's transport controls now dim (with a hint) during AirPlay
sessions that cannot be controlled remotely. Modern iOS runs no DACP
server for AirPlay 2 streams, so play/pause/next from the speaker
silently did nothing; the buttons now reflect reality and light up
automatically for senders that do support remote control.
