---
"boompi": patch
---

Bluetooth configuration is now identical on every board:
SecureConnections=off moves into the shared main.conf (it was pi3-only
for the bench box's counterfeit-CSR dongle, but any cheap dongle can
have the same defect, dongles migrate between boxes, and JustWorks
pairing has no MITM protection either way). The per-board rootfs
overlays now differ only in the model name.
