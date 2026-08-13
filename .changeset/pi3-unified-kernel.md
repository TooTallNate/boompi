---
"boompi": minor
---

The pi3 image now builds the same kernel config as the pi4
(bcm2711_defconfig - the config Raspberry Pi OS's kernel8.img uses to
boot Pi 3/3+/4/Zero 2 from one binary), the first structural step
toward a single unified image. The pi3's TF-A armstub is gone with
it: it existed to provide PSCI for the retired kexec trial mechanism,
and its fixed load addresses imposed a 24MB kernel ceiling the fatter
unified kernel would have hit. Stock firmware boot chain, spin-table
SMP. The change ships through the pi3's crash-safe PM_RSTS trial: a
candidate that fails to boot falls back to the old slot on its own.
