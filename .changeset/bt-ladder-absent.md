---
"boompi": patch
---

The Bluetooth dongle self-heal ladder now recovers a controller that
vanishes entirely, not just one that refuses to power on. The pi3
migration surfaced the gap: the 6.6.78 boot wedge can remove the hci
outright, and the old ladder both keyed off a present-but-unpowered
adapter and located the dongle by walking from the hci - so a dead
controller was invisible to it twice over. Recovery candidates now
also come from a USB device-class scan, stuck-disabled hub ports from
interrupted escalations are re-enabled first, adapter removal is
handled (clearing state and surfacing "unavailable" pairing), and a
30-second health tick retries on a loop that was previously purely
event-driven. Boxes with onboard or no Bluetooth stay quiet.
