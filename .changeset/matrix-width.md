---
"boompi": patch
---

The Matrix screensaver now fills any screen width: the rain computes
its column count from the display instead of assuming 800px (which
clipped the last column on the Pi 3 and left dead space on the Pi 4's
wider panel), and the column field centers itself.
