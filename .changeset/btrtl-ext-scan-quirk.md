---
"boompi": patch
---

Kernel patch: the fleet's Bluetooth dongle (RTL8761BU) claims HCI 5.1
but its firmware doesn't implement the LE extended scan commands,
producing EBUSY storms whenever BlueZ scans with a connection active.
Mainline Linux fixed this after our kernel's release by quirking the
chip back to legacy scan commands; the image now carries that fix as
a backport. One more entry in this chip's lying-about-its-features
rap sheet - and this one is upstream-certified.
