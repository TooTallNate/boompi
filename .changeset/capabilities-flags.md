---
"boompi": minor
---

Boxes now tell clients what they can do. The hosted remote and the
upcoming iOS app outlive any given box's software, so the connection
greeting grew a capabilities list (wifi, wifi_scan, battery,
bluetooth, games, ...) and the UIs hide what a box doesn't have:
connect to a speaker whose software predates Wi-Fi-over-Bluetooth and
the Wi-Fi page says so instead of scanning into the void; connect to
a hard-wired box with no battery sensor and the Battery page simply
isn't there. Hardware-dependent flags read live state, old boxes that
predate the field get a sensible legacy set, and unknown future flags
are ignored - so mismatched client/box versions degrade politely in
both directions.
