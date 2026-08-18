---
"boompi": patch
---

The Speaker name field (and the Home Assistant broker fields) no
longer show up empty on first load. They captured their initial value
before the live connection delivered the settings, and only a
remount refreshed them; untouched fields now always show the server's
value the moment it arrives.
