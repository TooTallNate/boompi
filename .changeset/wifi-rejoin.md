---
"boompi": patch
---

Saved-but-disconnected Wi-Fi networks now have an explicit Rejoin
button. Opening or polling the Wi-Fi page also no longer silently
undoes a deliberate Disconnect: NetworkManager can clear its own
autoconnect block as a side effect of scanning, so boompid records
the user's intent in /run and reasserts it after scans. Deliberate
Rejoin/connect, radio, hotspot, and forget actions clear the latch;
a reboot clears it naturally.
