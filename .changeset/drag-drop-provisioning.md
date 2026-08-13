---
"boompi": minor
---

Drag-drop provisioning from any OS: flash the generic image, copy a
boompi-box/ directory (the box profile) onto the boot partition your
OS mounts, and boot. The appliance ingests the bundle into /data/box/
on startup - before boompid launches, so the hardware profile applies
immediately - merges the firmware config into both boot slots,
renames the bundle *.applied (drop a fresh one to re-provision), and
reboots once only if the active boot config actually changed.
scripts/provision-sd.sh packages a boxes/ profile onto a mounted card
for convenience; the manual copy works identically.
