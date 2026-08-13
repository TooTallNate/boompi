---
"boompi": minor
---

The pi3/pi4 images are now board-generic: everything specific to one
physical build (display overlay, rotation, DAC HAT, panel video mode,
battery wiring, UI scale seed) moved out of the images into box
profiles (/data/box/, worked examples in boxes/). An unprovisioned
image boots to a recovery posture - HDMI, onboard Bluetooth/audio,
ssh, web settings - and profile-dependent features explain what is
missing. Profiles also carry kernel arguments now (cmdline.txt
fragment, e.g. the video= mode for an EDID-less panel), and a
profile's [settings] only seeds the first boot so user choices are
never clobbered. scripts/provision.sh provisions a running box over
ssh; docs/PROVISIONING.md documents the design.
