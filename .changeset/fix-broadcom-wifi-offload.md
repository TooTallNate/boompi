---
"boompi": patch
---

Fix onboard Wi-Fi authentication timing out after the Buildroot 2026.08
upgrade. Disable Broadcom firmware handshake offload so wpa_supplicant
can complete WPA2 authentication with the pinned Raspberry Pi kernel.
