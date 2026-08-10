---
"boompi": patch
---

Fix the unique-hostname unit failing on freshly written OS slots: it
ran while the rootfs was still read-only, the /etc/hostname write
failed silently, and NetworkManager later reverted the hostname to
the stale default - which also re-registered the speaker in Home
Assistant as a duplicate device after an update. The unit now waits
for the rw remount, and the MQTT device identity derives directly
from the SoC serial so it can never follow a stale hostname.
