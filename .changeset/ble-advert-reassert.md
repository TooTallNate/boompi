---
"boompi": patch
---

The speaker no longer vanishes from the Bluetooth choosers. The
fleet's TP-Link UB500 dongle (RTL8761B) silently stops broadcasting
its LE advertisement after enough connect/disconnect activity, while
BlueZ still reports it active - so the box would drop out of the
hosted remote's and iOS app's discovery until something re-registered
the advert (diagnosed live: an A2DP-connected box with an "active"
advert that no scanner could see, cured instantly by re-registering).
boompid now re-asserts the advertisement every minute; the cycle is
cheap and harmless to connected clients, and a dead broadcast heals
within a minute instead of never.
