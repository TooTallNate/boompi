---
"boompi": patch
---

Two reliability fixes from a bench incident: AirPlay audio could go
permanently silent because its PCM pipe lived in /tmp, where the
daily tmpfiles clean could reap it (the boxes boot with a months-old
clock until NTP lands, so boot-created files look ancient) - it now
lives in /run/boompi, which is never age-cleaned. And the Bluetooth
dongle recovery no longer USB-resets a truly-dead dongle every four
seconds forever: resets back off exponentially (up to 10 minutes),
recover an interrupted reset that left the dongle de-authorized, and
never strand the device in the off state.
