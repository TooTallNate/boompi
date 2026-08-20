---
"boompi": patch
---

The full hardening batch from the root-slot migration campaign. The
migration script now syncs the filesystem shrink to media before any
partition-table work (a kernel partition resize silently discards
unflushed page-cache writes - the pi4's shrink evaporated exactly
this way), verifies the table on disk instead of poking a live
kernel, and defers the root filesystem grow to a new boot-time
grow-root service. Recovery independence: a getty on tty2 (USB
keyboard + Ctrl-Alt-F2), NetworkManager runs without /data so wired
DHCP always works, and /data is fsck'd before mounting. No failure
of the data partition can strand the box unreachable again.
