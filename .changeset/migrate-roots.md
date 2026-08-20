---
"boompi": patch
---

Groundwork for 1GiB root slots. New boompi-migrate-roots grows both
A/B root partitions from 512MiB to 1024MiB in place - no reflash, no
data loss: /data's filesystem shrinks from its end, root-b is reborn
as the last GiB of the card, root-a absorbs its old neighbor. Proven
against loopback replicas of both fleet layouts (including the pi4's
legacy packed table) on real hardware and in CI. Updates now refuse
images larger than their slot instead of silently overflowing into
the neighbor partition, and grow-data learned to measure free space
from the last partition on disk. Images still build at 512MiB; the
size bump lands after the fleet migrates.
