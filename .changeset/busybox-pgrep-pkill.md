---
"boompi": patch
---

pgrep and pkill exist on the box now. Buildroot's default busybox
config omits them, and every bench-debugging session rediscovered
that the hard way ("sh: pkill: not found" while trying to stop a dev
daemon). A busybox config fragment turns them on.
