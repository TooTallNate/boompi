---
"boompi": patch
---

CPU temperature is no longer a Home Assistant exclusive. The box
broadcasts its thermal state over the protocol (30s cadence, on
change), so the web UIs show it on the General page and the iOS app
in General > About - along with a live "throttled" warning whenever
the firmware is actively limiting the clock from heat or a sagging
power supply, the invisible condition that once cost a full bench
session to diagnose. MQTT keeps publishing the same reading for HA.
