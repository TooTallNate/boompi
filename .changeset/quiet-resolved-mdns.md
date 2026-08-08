---
"boompi": patch
---

Fix vanishing AirPlay/Spotify adverts: systemd-resolved's built-in
mDNS responder fought avahi for the speaker's .local hostname, which
could leave avahi renaming itself in an endless loop (no service
adverts at all) or advertising under a shifted name that broke
AirPlay connects. resolved now leaves multicast DNS entirely to
avahi.
