---
"boompi": patch
---

Bluetooth volume is now correct, confirmed by measurement. The
speaker keeps absolute-volume negotiation on (the phone sends volume
commands that drive the music track while streaming constant-level
audio), the "stuck very quiet" state turned out to be a stale bond -
re-pairing anchors the session at full level - and a bench-calibrated
+4.3dB makeup gain on the Bluetooth stream makes identical content
measure identically loud across Bluetooth, AirPlay, and Spotify
(the latter two already agreed to 0.1dB).
