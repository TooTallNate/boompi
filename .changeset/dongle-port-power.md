---
"boompi": patch
---

Bluetooth dongle recovery now escalates to a USB port power cycle
when softer resets fail - the boot-time wedge some kernels produce
(HCI dead from second ten) previously required physically power
cycling the whole box; it now self-heals in about fifteen seconds
(sibling USB devices briefly re-enumerate). Touch ripples also land
under the finger on rotated panels now instead of at mirrored
positions.
