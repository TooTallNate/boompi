---
"boompi": patch
---

Pairing an iPhone while a gamepad is connected no longer ends in
"Connection Unsuccessful". When A2DP setup runs long (a busy radio -
gamepad traffic, pairing bursts, and USB audio all share one bus on
the Pi 3), the box now brings the audio profile up over the existing
link instead of disconnecting the phone mid-setup, and it stops
poking the phone's hands-free profile a speaker can't answer anyway.
