---
"boompi": minor
---

On-box low-battery safeguard. The panel shows a warning banner (and
wakes the screensaver) when state of charge drops to 15%, clearing
with hysteresis or whenever the charger is connected. If SoC falls to
5% or the pack holds below 18.3V for a sustained 60 seconds while
discharging, the box announces itself on the panel and powers off
cleanly before the pack reaches the BMS cutoff - a deep discharge
corrupted an SD card once already. Thresholds are configurable and
the auto-shutdown can be disabled in [battery] config. The web UI
shows the low state, and the MQTT battery payload carries it for
Home Assistant automations.
