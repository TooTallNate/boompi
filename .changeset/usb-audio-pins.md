---
"boompi": patch
---

Audio output paths get the same guard treatment as Bluetooth dongles:
kernel pins and post-build assertions for the USB Audio Class driver
(one driver covers essentially every USB sound card - no per-chipset
firmware, unlike Bluetooth) and the common I2S DAC HAT modules
(HiFiBerry-compatible machine drivers + PCM51xx codecs). A boombox
image that cannot make sound must not build.
