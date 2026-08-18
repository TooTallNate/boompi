---
"boompi": patch
---

The speaker stays visible in the Bluetooth choosers - including while
a remote is already connected. Two quirks of the fleet's TP-Link
UB500 dongle (RTL8761B) shaped this: it silently stops broadcasting
its LE advertisement after connect/disconnect churn while BlueZ still
reports it active, and cycling the advertisement registration while a
client is connected drops that client's connection. So boompid now
re-asserts the advertisement on a timer only while idle (healing the
silent death), and the moment a remote connects it registers a spare
advertising instance instead - the controller runs three, so a second
remote can still discover and connect. Verified live: the iOS app and
the web remote controlling the same speaker simultaneously, changes
reflecting in both directions, while the box stays discoverable.
