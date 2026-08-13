---
"boompi": patch
---

Images now ship Realtek Bluetooth USB dongle firmware (RTL8761B/BU:
TP-Link UB500, ASUS USB-BT500, newer UB400 revisions) so a
recommended dongle works the moment it is plugged in - previously it
would enumerate but hci0 would never appear. The Realtek btusb kernel
option is pinned by fragment, and a post-build assertion guards the
firmware files. Comments no longer call the pi3's dongle counterfeit:
it is a genuine TP-Link UB400 whose CSR8510 chip (BT 4.0) simply
predates Secure Connections while advertising it.
