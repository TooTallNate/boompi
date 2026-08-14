---
"boompi": patch
---

The RetroArch menu now rotates with the panel instead of rendering
sideways. Upstream RetroArch has no way to rotate the menu on KMS
displays (the screen-orientation backend is an unimplemented stub),
so the image carries a small patch adding a `menu_rotation` setting -
menu-only, never combined with the rotation a game requests - which
boompid sets from the same box profile as the game rotation.
Touchscreen taps in the menu are remapped to match.
