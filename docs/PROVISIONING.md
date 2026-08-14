# Box provisioning

The OS images are **board-generic** (one per SoC: pi3, pi4). Anything
specific to one physical build - display overlay, rotation, wiring,
battery bus, amp GPIO - is a **box profile** that lives on the `/data`
partition and survives OS updates and factory resets.

An unprovisioned image boots to a useful recovery posture: HDMI
console, onboard Bluetooth/audio, ssh, and the web settings page.
Profile-dependent features explain what is missing instead of hiding
(e.g. the battery screen shows the exact `hardware.toml` snippet to
add).

## The profile: `/data/box/`

| File | Consumed by | Contents |
|---|---|---|
| `config.txt` | firmware (via re-materialization) | dtoverlays, dtparams, gpio lines |
| `cmdline.txt` | kernel (single line, appended) | e.g. `video=` for an EDID-less panel |
| `hardware.toml` | boompid (`--hardware-profile`) | `[battery]` wiring/thresholds; `[settings]` seeds first boot only |
| `env` | boompi-ui (`EnvironmentFile`) | e.g. `SLINT_KMS_ROTATION=270` |
| `authorized_keys` | sshd (installed to `/data/ssh/`) | ssh public keys; see docs/SECURITY.md |

All files are optional. The bench boxes' profiles live in `boxes/` in
this repo and double as worked examples.

## How the firmware config survives updates

The Pi firmware cannot `include` across partitions, so
`boompi-apply-box-config` copies the fragment into a fenced section of
a boot partition's `config.txt` (and appends `cmdline.txt` after a
`boompi.box` marker - the dot makes the kernel treat it as a module
parameter and stay silent). The fence is replaced wholesale on every
apply, so the operation is idempotent.

Everything that writes a boot partition re-applies the profile:

- the on-box updater (before arming the A/B trial; a failure aborts
  the update, because a candidate booting without its display overlay
  would pass the sick-check with a dark panel),
- `boompi-update-slot` (ssh-driven updates),
- `boompi-apply-box-config --all` (manual, after editing the fragment;
  reboot to take effect).

The A/B trial protects profile changes the same way it protects OS
updates: a new slot that fails to boot rolls back to the old slot with
the old merged config.

## Provisioning a box

From the box itself (the primary path): the web settings page has a
"Box hardware" page (`#/hardware`) - the configurator ships in the
same image as the code that consumes the profile, so the two cannot
drift. Pick a preset or edit the fragments, add your ssh public key,
Apply (written to `/data/box/` and fenced onto both boot slots; one
reboot when the boot config changed), and "Download bundle" produces
the `boompi-box.tar` for provisioning the next card. When the box is
set up, **Lock** the page: hardware config becomes ssh-only
(`boompi-box`; export included for locked boxes) - see
docs/SECURITY.md.


Fresh SD card, from any OS (no root, no ext4 tooling): flash the
generic image, let the OS mount the boot partition, then

    scripts/provision-sd.sh georges /Volumes/bootfs

(or just copy a `boompi-box/` directory with the profile files onto
the FAT by hand - the script is a convenience). On first boot the
appliance ingests the bundle into `/data/box/`, merges the firmware
config into both boot slots, renames the bundle `*.applied`, and
reboots once if the active boot config changed. Drop a fresh bundle
any time to re-provision.

Running appliance, over ssh:

    scripts/provision.sh georges root@192.168.1.118

This writes `/data/box/` and restarts boompid. The firmware fragment
lands on the boot partitions at the next OS update - or immediately
with `--apply` (only on boxes already running a board-generic image;
on the old tailored images the fragment would duplicate the baked-in
config).

`[settings]` in `hardware.toml` is a *seed*: it applies only until the
runtime config (`/data/boompi.toml`) exists. The user's later choices
win. Hardware tables (`[battery]`, ...) always win - wiring is not a
preference.

Factory reset keeps `/data/box/` - resetting a speaker does not change
its wiring.

## Per-box binaries: /data/bin

`/data/bin` is first in `$PATH` everywhere (interactive shells, ssh
commands, systemd services) and survives OS updates like the rest of
/data - the home for box-local tools (a Node.js runtime, scripts)
that do not belong in the base image. Being first also means a box
can shadow an image binary for local experiments; services started
by absolute path (boompid, boompi-ui) are unaffected. Root-only, not
exported over SMB, untouched by factory reset.

## Not yet built (roadmap)

- Trialing box-profile changes through the A/B machinery (fence the
  inactive slot, one-shot boot it, commit) so even a boot-breaking
  dtoverlay cannot strand a box.
- A hosted flavor of the configurator (the web UI is a static bundle;
  a "no live box" build could go on GitHub Pages) for people flashing
  their very first card.
