---
"boompi": patch
---

The pi3 and pi4 images now differ only in board facts (kernel,
firmware, TF-A). The vestigial model config key is gone (the Hello
handshake reports the device-tree model string instead), the
per-board rootfs overlays are deleted, onboard Bluetooth UART
firmware ships on both boards (the generic pi3 image left onboard BT
enabled but never shipped its .hcd), and the post-build assertions
now check the real A/B mechanism (PM_RSTS/autoboot tooling and the
box-profile apply script) instead of the retired kexec - plus both
BT firmware families unconditionally, replacing a pi4-only check
that had gone silently dead.
