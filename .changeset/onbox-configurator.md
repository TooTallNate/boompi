---
"boompi": minor
---

The configurator lives on the box: a "Box hardware" section in the
web settings UI edits the box profile live - presets for the known
builds, editors for the firmware fragment, kernel arguments, hardware
TOML, and panel environment. Apply writes /data/box/, re-fences both
boot slots, and prompts a reboot only when the boot config actually
changed; Download packages the profile as the boompi-box.tar bundle
for provisioning the next SD card. Shipping the configurator inside
the image it configures means the profile schema and its editor can
never drift apart. Validation refuses the foot-guns (root= overrides,
multi-line cmdline, fence markers, unparseable TOML).
