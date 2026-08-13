---
"boompi": minor
---

One image for every board, and two release assets total. The pi3/pi4
builds collapse into a single board-generic image: one kernel
(bcm2711 config, proven on the Pi 3 by A/B trial), both GPU firmware
sets and DTBs on each boot slot, [pi3]/[pi4] conditional sections in
config.txt, and the box profile carrying everything hardware-specific
as before. Releases now publish exactly a flashable sdcard image and
one self-contained update bundle (boompi-update.tar: checksums and
version stamp first, then the zstd payloads) which the updater
consumes as a single stream, routing what it needs onto the inactive
slot's partitions and skipping the rest. The Pi 3 trial-boot arming
now works on both kernel eras: TF-A kernels preserve a pre-written
PM_RSTS through BL31, stock kernels get the partition via the reboot
syscall argument (the restart handler otherwise clobbers PM_RSTS -
bench-falsified and fixed the same night).
