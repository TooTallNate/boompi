---
"boompi": patch
---

Boxes on the bleeding-edge channel now check for updates every 10
minutes instead of every 6 hours - a green build lands with most
pushes, and the whole point of opting in is riding the front of the
wave. Stable stays at 6 hours, and flipping the channel toggle takes
effect at the next wakeup without a restart.
