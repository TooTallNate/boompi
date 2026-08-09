# The A/B update system

How boompi boxes update themselves, and why the mechanism is different
on each board. This documents what actually shipped; the bench
falsified most of the original design assumptions (including the
official documentation's) along the way.

## Layout

Both boards use the same SD card layout:

```
p1  boot-a   FAT32: firmware, kernel, cmdline-a (root=p3), autoboot.txt
p2  boot-b   FAT32: firmware, kernel, cmdline-b (root=p5)
p3  rootfs-a ext4
p4  (extended)
p5  rootfs-b ext4
p6  data     ext4 - config, pairings, caches, fonts; never touched by updates
```

The firmware reads `autoboot.txt` on p1 (`boot_partition=N`) to pick
the default slot. An update writes the candidate into the inactive
slot, verifies it, trial-boots it once, and only makes it the default
after the system proves healthy (`boompi-boot-commit` gates on
boompid's `/healthz`).

## Trial mechanisms (the interesting part)

| | Pi 3 (BCM2837) | Pi 4 rev ≤ 1.3 (BCM2711) |
|---|---|---|
| mechanism | one-shot PM_RSTS partition request | autoboot flip + sick-rollback |
| crash during trial | any reset boots the old slot | stuck on candidate (SD edit) |
| boots-but-sick | commit script reboots to old slot | commit script flips back |

**Pi 3**: the pre-tryboot `reboot N` protocol. The target partition
number, spread into PM_RSTS bits 0,2,4,6,8,10 with the `0x5a`
password byte, survives a warm reset; bootcode.bin boots that
partition once and clears the request. autoboot.txt keeps pointing at
the old slot until commit, so every failure mode falls back.

**Pi 4 rev ≤ 1.3**: no one-shot mechanism exists. The board
power-cycles its PMIC on every reboot, wiping PM_RSTS and the
firmware tryboot flag (both bench-verified lost on rev 1.2 with the
newest EEPROM). So the update flips autoboot.txt to the candidate
BEFORE rebooting and `boompi-boot-commit` flips it back if the
candidate boots unhealthy. The residual risk - a candidate kernel
that never boots leaves the box on the broken slot - is accepted
because slots are sha256-verified after writing and both boards run
the same userspace (the Pi 3, with true fallback, is the natural
canary). Rev 1.4+ boards keep PM_RSTS through reboots and could use
the Pi 3 path.

**Retired: kexec.** The original design chain-loaded the candidate
kernel with kexec to avoid touching firmware state at all. It hangs
after the old kernel's "Bye!" whenever the candidate kernel is a
different build from the running one - first written off as a Pi 3
quirk, then reproduced on the Pi 4 during the v2.0.0 rollout. The
best available model: an instruction-cache coherency race in the
handoff (byte-identical kernels are immune because stale cache lines
contain the right bytes anyway; differing builds roll dice weighted
by how much the code layout moved).

**Retired: firmware tryboot.** The documented mechanism
(`reboot "0 tryboot"`, `[tryboot]` filter in autoboot.txt) does not
survive on either board: on BCM2837 the mailbox-set flag lands in
PM_RSTS bit 1, which the watchdog reset clears (verified with the
flag armed and a raw register-poked reset); on Pi 4 rev ≤ 1.3 the
PMIC power-cycle wipes it. The docs' "all models support tryboot"
carries asterisks the bench had to discover.

## Delivery

- **Releases** (changesets flow): merging the Version Packages PR
  publishes `vX.Y.Z` with sdcard images + per-board OTA assets
  (zstd-compressed rootfs/boot images + SHA256SUMS.txt).
- **Edge**: every green build of `main` replaces the rolling `edge`
  prerelease with the same asset contract. GitHub release assets are
  used (not CI artifacts) because artifacts cannot be downloaded
  anonymously.
- **On-box updater** (`boompid update.rs`, Settings → Software):
  checks the selected channel on demand and every 6 h, streams the
  zstd assets straight into the inactive slot's partitions (nothing
  on a 1 GB box can stage a 640 MB bundle), hashes the decompressed
  stream, re-reads the partition against SHA256SUMS, then arms the
  trial via `boompi-trial-boot`.
- **Workstation** (`scripts/update-appliance.sh`): same flow driven
  over SSH from CI artifacts; `TRIAL=0` skips the trial for
  commit-without-trial.
- Images are stamped (`/etc/boompi-version`): clean `vX.Y.Z` on the
  version-bump commit a release is cut from, `vX.Y.Z-<sha>` on every
  other build. The stamp describes the OS image in the slot, not the
  boompid binary - hand-deployed dev binaries don't change it, so the
  updater's decisions stay truthful.
