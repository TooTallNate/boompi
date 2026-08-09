---
"boompi": patch
---

Now-playing layout always fits the screen: at larger text sizes the
volume slider could overflow into the footer and become unreachable.
The AirPlay "controls not supported" hint no longer occupies layout
space - disabled transport buttons now show a brief toast when tapped
instead. Volume slider drags are also throttled (~10 updates/s with
the final position always applied), so the level tracks the finger
instead of crawling after it.
