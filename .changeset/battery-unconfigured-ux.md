---
"boompi": patch
---

UIs explain an absent battery instead of hiding it. The panel's
footer battery icon is always visible (empty outline without
telemetry) and the battery screen distinguishes "not configured"
(with the exact /data/box/hardware.toml snippet to add) from
"sensor not responding" (with the probe error). The web settings
page shows the same guidance. Groundwork for board-generic images,
where a fresh unprovisioned box is a normal state rather than a
mystery.
