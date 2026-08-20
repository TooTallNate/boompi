# In-place migration to 1024M root slots

`boompi-migrate-roots` grows both A/B root partitions from 512M to
1024M on a live box - no reflash, no data loss, no SD card extraction.
The pi3 was migrated 2026-08-20; this runbook codifies that experience
for the pi4 (and any future box). Read it start to finish before
touching anything: every step below exists because something bit us.

## How the migration works (one screen)

```
before:  p1 p2 | p3 root-a 512M | p5 root-b 512M | p6 data → card end
after:   p1 p2 | p3 root-a 1G ——————————————————| p6 data ...| p5 root-b 1G
```

- /data's filesystem shrinks from its END (seconds; no data blocks
  move; the start sector is never touched).
- root-b is recreated as the last 1024M of the card. The MBR logical
  chain goes out of physical order (p5 beyond p6): the kernel numbers
  logicals by EBR chain order, so every `/dev/mmcblk0pN` literal in
  the fleet stays true, and `data.mount` is by-label anyway.
- root-a then grows into root-b's old home; its filesystem grow is
  deliberately deferred to `boompi-grow-root` on the next boot - the
  running kernel cannot adopt the resized mounted-root partition, so
  the script verifies the table ON DISK and reboots to adopt it.
- The tightest fit is the pi4's legacy packed table (p1 at sector 1,
  1-sector EBR gaps): a full-GiB root-a leaves exactly two spare
  sectors for the EBRs. Proven on real hardware and by `sfdisk -V`;
  CI (`migrate-test`) exercises loopback replicas of both fleet
  layouts on every build.

## Prerequisites (all must hold)

1. **Box is on slot A** (`root=/dev/mmcblk0p3` in /proc/cmdline).
   Old p5 is demolished; the script refuses to run from B.
2. **Box runs image ≥ 2626b8a** (migrate script aboard, fixed
   grow-data, fits-the-slot update guards). An older image's
   grow-data measures free space from p6's end and would fight the
   migrated layout on every boot.
3. **No trial pending** (`/data/boompi-trial` absent).
4. 512M *prep bundle* (2626b8a) and 1024M *final bundle* (≥ ddebb7b)
   downloaded locally. A 1024M image cannot be delivered to an
   unmigrated box - the updater refuses (that guard is load-bearing;
   do not bypass it).

## Step-by-step (pi4)

```sh
# 0. State check: slot, layout, health. EXPECT root=/dev/mmcblk0p3.
ssh $PI 'tr " " "\n" < /proc/cmdline | grep root=; sfdisk -d /dev/mmcblk0'

# 1. Bring the box to image >= 2626b8a ON SLOT A. The pi4 starts on A,
#    so this is TWO update cycles (each writes the inactive slot and
#    boots it): A -> B on the prep bundle, then B -> A on the same.
#    Verify health + THE PANEL after each boot.
PI=root@<pi4> BOARD=pi4 scripts/update-appliance.sh <prep-bundle-dir>
# ...wait, verify root=p5, panel up...
PI=root@<pi4> BOARD=pi4 scripts/update-appliance.sh <prep-bundle-dir>
# ...wait, verify root=p3, panel up...

# 2. Off-box backup (the script also backs up to /data, but /data is
#    exactly what you want a copy of OFF the box).
ssh $PI 'sfdisk -d /dev/mmcblk0; dd if=/dev/mmcblk0 bs=512 count=2048 | gzip' \
    > pi4-table-backup.gz

# 3. Dry run - writes nothing, prints the plan. EYEBALL IT against the
#    expected values below.
ssh $PI boompi-migrate-roots

# 4. Execute. The script re-execs itself into a transient systemd
#    unit (the quiesce kills sshd - see lessons), so your session
#    drops. Follow from a second session until the box reboots itself.
ssh $PI 'BOOMPI_MIGRATE_CONFIRM=yes boompi-migrate-roots'
ssh $PI 'journalctl -u boompi-migrate -f'      # until connection drops

# 5. After its self-reboot: verify table, root fs size, /data, panel.
ssh $PI 'sfdisk -d /dev/mmcblk0; df -h / /data; systemctl is-active boompi-ui boompid'

# 6. Populate the reborn p5 with the 1024M bundle (this is also the
#    end-to-end validation of the new slot).
PI=root@<pi4> BOARD=pi4 scripts/update-appliance.sh <1024M-bundle-dir>
# ...wait, verify root=p5, df -h / reports ~991M, panel up. DONE.
```

Expected pi4 plan (32GB card, 62333952 sectors; the script derives
these from the live table - if the dry run disagrees, STOP):

```
p3 root-a : start 262145   size 1048576 -> 2097152
p4 ext    : start 1310721 -> 2359297    size -> 59974655
p5 root-b : start 1310722 -> 60235776   size -> 2097152
p6 data   : start 2359299 (UNCHANGED)   size -> 57868285
```

## Recovery

Every interruption state boots: p3's start sector never moves, boot
partitions are never written, autoboot keeps pointing at A, and the
table rewrite is one MBR sector + two EBRs. If the box goes dark
mid-surgery, power-cycle it and assess:

- `sfdisk -d` shows the OLD table → the fs shrink may or may not have
  happened (both fine); rerun the migration.
- Shows the NEW table → surgery landed; finish by hand if needed:
  `resize2fs /dev/mmcblk0p3`, then reboot.
- Table half-written / partitions missing → restore from backup:
  `zcat pi4-table-backup.gz | head -20` has the sfdisk dump; feed the
  partition lines to `sfdisk --force /dev/mmcblk0`, reboot.
- /data won't mount → `e2fsck -y /dev/mmcblk0p6` (an interrupted
  resize2fs shrink is repairable; worst case /data is config + caches).

## How the pi4 actually went (2026-08-20)

The pi4's first migration attempt failed and was recovered by reflash
+ identity restore - which is itself a documented path now:

- The failure: resize2fs's shrink lived only in the page cache when
  `partx` resized p6 in the kernel; a block-device resize INVALIDATES
  that device's page cache, so the shrink evaporated before reaching
  the SD card. The table (written synchronously) survived → new table
  around an unshrunk fs → /data unmountable → no boompid, no NM (hard
  Requires), no sshd keys, no getty anywhere: an unreachable box with
  a working panel. All five of those failure links are now fixed.
- The recovery (no working console needed, SD reader + Mac only):
  `brew install e2fsprogs`, extract the journal from p3 with
  `debugfs -R "rdump /var/log/journal ..."` (read it on another box),
  extract all of /data with `debugfs -c -R "rdump / payload"` (the
  fs/partition size mismatch doesn't block reads), flash the current
  `boompi-sdcard` artifact, drop a `boompi-box/` provisioning bundle
  (box profile + authorized_keys from the payload) on the boot FAT,
  boot, ssh in over wired DHCP, tar the payload back into /data, fix
  perms (600 keyfiles), reboot. Identity fully preserved - original
  ssh host key, wifi, bluetooth pairings, name.

## Lessons learned (the pi3 + pi4 migrations, 2026-08-20)

Each of these is now codified in code; listed so nobody un-learns them:

1. **Quiescing /data kills your ssh session.** NetworkManager and
   sshd have `RequiresMountsFor=/data` (keyfiles, host keys); systemd
   propagates the `data.mount` stop to them. The first migration ran
   as a child of the ssh session and died with it, mid-flight,
   leaving the box networkless (power cycle + hand-finish recovered
   it). → the script now self-detaches into a transient unit
   (`systemd-run`) and ends with a reboot.
2. **The workstation updater must re-apply the box profile.** Bundle
   boot images are board-generic; `boompi-apply-box-config` merges
   the box's display/hardware config into the fenced config.txt
   section. `update-appliance.sh` skipped it (the on-box paths never
   did) - two updates in one day left BOTH boot partitions generic:
   no panel framebuffer, boompi-ui crash-looping. → applied after
   every boot-partition write, all trial paths; always verify the
   panel after an update.
3. **pi3 one-shot trials need both arming mechanisms.** The devmem
   PM_RSTS write is discarded by spin-table kernels' restart handler;
   `systemctl reboot --reboot-argument=N` is discarded by PSCI
   kernels. `update-appliance.sh` only did devmem and the box booted
   straight back into the old slot. → both, mirroring on-box
   boompi-trial-boot. (pi4 uses autoboot-flip; not affected.)
4. **Old grow-data fights the migrated layout.** It measured free
   space from p6's end; with p5 living beyond p6 it would try to grow
   data over root-b every boot. → gap now measured from the last
   partition on disk; which is why the box must run ≥ 2626b8a
   *before* migrating.
5. **A 512M image in a 1024M slot reports 512M.** The rootfs is a
   fixed-size ext4 image; the partition being bigger changes nothing
   until `resize2fs`. The migration script grows p3 itself; a slot
   written by an OTA carries whatever the image was built at (the
   1024M images fill the slot natively).
6. **Update bundles cannot fix a box they can't fit into.** The
   fits-the-slot guards (update.rs, boompi-update-slot) refuse
   oversized images with instructions instead of dd'ing over the
   neighbor partition. This makes the migration a hard prerequisite
   for 1024M-era updates - by design.
7. **`sync` before touching the partition table - and on every exit
   path.** resize2fs writes through the block device's page cache; a
   kernel partition resize invalidates that cache, DISCARDING dirty
   pages. Zero seconds elapsed between shrink and partx on the pi4;
   the shrink never reached the card. (The pi3 survived the identical
   sequence only because its script died at a luckier instant and the
   30s writeback timer beat the power cycle.) → the script syncs
   after the shrink, before sfdisk, and in `fail()`.
8. **Never partx a live disk you're rearranging.** It cannot resize
   the mounted root or a moved partition, and its partial success is
   what triggers the cache invalidation. → verify the on-disk table
   (`sfdisk -d` re-read) and reboot to adopt; `boompi-grow-root`
   grows the root fs on the following boot.
9. **Every recovery layer must not share a single point of failure.**
   sshd's keys, NM's credentials, and boompid's config all lived
   behind one mount, and the console didn't exist. Now: getty on
   tty2 (USB keyboard + Ctrl-Alt-F2, root/console password), NM
   starts without /data (wired DHCP always works), e2fsck -p runs
   before data.mount, and boompi-grow-root self-heals undersized
   root filesystems.
