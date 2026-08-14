---
"boompi": patch
---

Display rotation is now declared exactly once - in the box profile's
device tree (`dtparam=rotate=`). The panel UI and the game launcher
read the kernel's DRM panel-orientation hint instead of carrying
their own copies, the boot console rotates to match (sideways kernel
panics are finally readable), and `SLINT_KMS_ROTATION` in the env
profile becomes an optional override rather than a requirement.
