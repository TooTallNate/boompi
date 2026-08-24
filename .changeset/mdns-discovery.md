---
"boompi": patch
---

Boxes now advertise a `_boompi._tcp` DNS-SD service on the LAN via
avahi, so control clients (the iOS app, web remote, other boompis)
can discover boxes by name instead of needing an IP. The instance
name is the speaker name (full UTF-8, following renames live), and
TXT records carry the connection contract: stable box id, WebSocket
protocol version, OS image version, and the /ws path on the
advertised port. A %h-wildcard baseline in the image covers first
boot before boompid's first write.
