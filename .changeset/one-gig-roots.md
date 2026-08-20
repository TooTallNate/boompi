---
"boompi": minor
---

Root filesystems are now 1024MiB (doubled from 512MiB). The 512M slots
had ~100MB of headroom left for new features; both boxes' partitions
were grown in place by boompi-migrate-roots - no reflash, no data
loss, no SD card extraction. Images from this release require migrated
(or freshly flashed) root slots; the updater refuses delivery to
unmigrated boxes with instructions rather than risking the neighbor
partition.
