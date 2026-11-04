---
"boompi": minor
---

Move the kernel from the Raspberry Pi 6.6 branch, which stopped
receiving updates in March 2025, to the maintained rpi-6.18.y branch
(6.18.54). Local Bluetooth and boot-logo patches are rebased. The
TP-Link UB600 dongle support is now upstream. A Broadcom Wi-Fi driver
fix for WPA2 handshake offload is backported. The existing Wi-Fi
workaround stays in place until that fix is validated on both boards.
