---
"boompi": minor
---

Wi-Fi management no longer needs an IP path. Scanning and
password-joins now ride the protocol (`scan` answers with a
`wifi_networks` broadcast, `connect` carries the psk), so the
Bluetooth remote at boompi.n8.io manages Wi-Fi exactly like the box's
own settings page: see nearby networks with signal strength, join new
ones, disconnect, forget, toggle the radio and hotspot - all over the
radio, which conveniently survives the Wi-Fi changes it causes. Join
progress broadcasts the same way the setup wizard's does. The REST
endpoint stays as the synchronous-error flavor the on-box web app
prefers, but the two paths now render one identical Wi-Fi UI.
