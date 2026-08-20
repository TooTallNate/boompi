---
"boompi": patch
---

Uptime in General → System now ticks live (30s cadence, derived from
the handshake snapshot plus elapsed wall time - a reconnect after a
reboot resets the baseline) and spells out its units: "2 days 5 hr
42 min" instead of a four-digit pile of minutes.
