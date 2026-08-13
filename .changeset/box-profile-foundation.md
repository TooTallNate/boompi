---
"boompi": minor
---

Foundation for board-generic images: box-specific configuration now
has a home that survives OS updates. A box profile in /data/box/ can
carry a firmware config fragment (config.txt - display overlay,
rotation, wiring, amp GPIO), a hardware.toml merged over the boompid
config, and an env file for the panel service (e.g. rotation). The
firmware fragment is re-materialized into a fenced section of
config.txt whenever a boot partition is written - by the on-box
updater and by boompi-update-slot - so a box keeps its identity
across A/B updates. Boxes without a profile behave exactly as
before; extracting the two bench boxes' specifics out of the pi3/pi4
images comes next.
