---
"boompi": patch
---

Out-of-box support for the common Bluetooth USB dongle chipset
families, befitting a generic image. The TP-Link UB600 turned out to
be the same RTL8761BU as the UB500 hiding under TP-Link's own USB
vendor id, which the pinned 6.6 kernel does not map to the Realtek
firmware loader - hci0 appears but scans find nothing. The upstream
fix (v7.2-rc1) is backported as a kernel patch. Firmware coverage
grows to Realtek combo adapters (8821/8822/8852) and MediaTek
MT7921/MT7922, with post-build assertions per family.
