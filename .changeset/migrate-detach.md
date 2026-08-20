---
"boompi": patch
---

boompi-migrate-roots now detaches itself into a transient systemd unit
before surgery and reboots when done. Quiescing /data cascades into
NetworkManager and sshd, which kills the ssh session that launched the
script - and on the pi3's first live migration, the script died with
it, mid-flight (the surgery had luckily already landed; a power cycle
recovered everything). Also: the workstation updater now arms the pi3
one-shot trial via both PM_RSTS and the reboot argument, matching the
on-box script - devmem alone is discarded by spin-table kernels.
