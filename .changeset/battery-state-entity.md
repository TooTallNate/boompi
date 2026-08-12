---
"boompi": patch
---

Home Assistant gains a Battery state sensor (full / charging /
discharging / idle). The full detection already existed on the panel
and in the payload, but HA only had a charging binary - so chargers
that terminate and periodically top the pack back up (rather than
holding a float) looked like they cycled forever without finishing.
