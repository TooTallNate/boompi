---
"boompi": patch
---

N64 works on both boxes now. The recompiler build that runs on the
Pi 3 turned out to crash on the Pi 4's different CPU core (verified
with the same binary on both boards), so the image now ships
per-board builds of the N64 emulator - the launcher picks the right
one automatically - and the Pi 4 variant gets the nicer GLES3
graphics path as a bonus.
