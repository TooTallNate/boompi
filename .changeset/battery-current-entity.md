---
"boompi": patch
---

Home Assistant gains a Battery current sensor (amps, signed - it goes
negative while charging), and all entities now declare device-scoped
names so newly added ones get clean entity ids instead of a doubled
device prefix.
