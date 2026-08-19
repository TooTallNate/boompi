---
"boompi": patch
---

Bluetooth on the boxes got dramatically calmer. The advertising
keep-alive was re-registering every 15 seconds, which the UB500's
controller handles badly: a stream of "unexpected advertising set
terminated" kernel events, EBUSY races, and disturbed connections and
pairing while it churned. Re-assertion is now event-driven - it fires
the moment a remote disconnects (the case that actually leaves the
broadcast dead) with a 5-minute safety net behind it, instead of
hammering the radio on a timer.
