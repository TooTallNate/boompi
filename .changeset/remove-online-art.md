---
"boompi": patch
---

The "fetch album art online" toggle is gone from every settings UI.
It shipped as a switch without an implementation behind it - a
promise the box never kept - and the direction is to make the real
art paths (AVRCP cover art from the phone) work well instead of
papering over them with a network fallback.
