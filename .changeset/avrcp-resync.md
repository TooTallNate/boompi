---
"boompi": patch
---

Track info and cover art no longer freeze mid-session. Phones
(iOS especially) silently drop the AVRCP control channel while A2DP
audio keeps streaming, and a missed D-Bus event could leave boompid
showing the same song and cover forever - skipping tracks did nothing
because the box never heard about them. A 30-second reconciliation
sweep now compares believed state against BlueZ's actual object tree:
missed players get adopted, vanished players clear the stale track,
and a connected phone that lost its control channel gets the profile
actively reconnected (verified to resurrect metadata live). Art
fetches follow track changes again, as they always should have.
