---
"boompi": minor
---

Connected clients now keep the clock honest when the internet can't.
The boxes have no RTC, so off-network the clock drifts months into
fantasy the moment NTP is unreachable. Every client that connects
already knows the time, so now they offer it: the web app sends its
clock on every WebSocket connect, and native apps can write the same
`set_time` message to the BLE control characteristic (documented in
docs/BLE.md for the upcoming iOS app). The offer is applied only when
timesyncd reports the clock was never NTP-disciplined this boot,
implausible values are rejected, and NTP silently overwrites
client-set time whenever it becomes reachable again - a working
internet connection always wins.
