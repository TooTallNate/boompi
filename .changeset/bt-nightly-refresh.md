---
"boompi": patch
---

Bluetooth gets a nightly immune-system reset. The fleet's USB dongle
has now been caught three times silently losing controller state -
advertising broadcasts, and most recently the classic device name
(the box became invisible to phones trying to pair, while every
setting read back correct). Each flavor got a targeted fix, but the
pattern predicts more, so: every night at 3:30 the Bluetooth stack
restarts and rewrites every controller register from scratch - only
when nothing is connected (an active music session or game controller
skips the refresh until the next night), and the box's services
re-register automatically like they already do. Any state rot, known
or not-yet-discovered, now lives at most a day.
