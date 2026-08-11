---
"boompi": patch
---

The CPU temperature sensor in Home Assistant now updates every
minute - previously it only published when the MQTT session
(re)connected, leaving hours-wide gaps in the history graph.
