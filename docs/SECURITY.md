# Security posture

Boompi is a LAN appliance: no TLS, no accounts, and a web settings
page anyone on the network can open. The posture below separates
what's harmless (volume, screensavers) from what can degrade the box
(boot configuration) and what can only be trusted to ssh.

## Surfaces

| Surface | Auth | Can do |
|---|---|---|
| Web settings page / `/api/*` | none (LAN) | everyday settings, updates, Wi-Fi, pairing |
| Web hardware page / `/api/box` | none **until locked** | boot config, ssh key install, provisioning bundle |
| SMB `smb://<box>/games` | none (LAN, guest) | read/write the games library only |
| ssh (root) | **public key only** | everything, incl. `boompi-box`, factory reset |
| Console (HDMI + keyboard) | root password | everything (physical access) |

## SSH

- Key-only: `PasswordAuthentication no`, `PermitRootLogin
  prohibit-password`. The image ships trusting **nobody** - there are
  no baked authorized keys.
- Per-box keys live at `/data/ssh/authorized_keys` (surviving OS
  updates like the host keys next to them). Ways in:
  - `boompi-box/authorized_keys` in the flash-time bundle
  - the web hardware page (before locking)
  - `scripts/provision.sh` / `provision-sd.sh` (default to your own
    `~/.ssh/id_*.pub`)
  - `boompi-box add-key` on the console

## The hardware lock

The web hardware page can rewrite the boot configuration - a wrong
display overlay means a dark panel. After a box is set up, **lock
it**: the page and `/api/box` answer 403, and hardware changes become
ssh-only (`boompi-box`). Locking requires an authorized ssh key
first, so the lock can never remove your last remote path in.
Unlock: `boompi-box unlock`.

Factory reset is ssh/console-only by design (`boompi-factory-reset`);
it is deliberately absent from the web UI and network APIs.

## The console password

The root password is `boompi` and works **only on the HDMI console**
(sshd refuses passwords). It is the second-to-last rung of the
recovery ladder and is deliberately not treated as a secret: physical
access to the box (and its SD card) is game over regardless. Changing
it (`passwd` on the console) does not survive OS updates - the rootfs
is replaced wholesale by A/B updates - which is acceptable for a
console-only credential. If that bothers you, your threat model
includes people in your living room.

## Recovery matrix

| State | Recovery |
|---|---|
| Dark panel, ssh key provisioned | ssh → `boompi-box` |
| Dark panel, no key, unlocked | web `#/hardware` page |
| No key and locked | HDMI console (root / `boompi`) |
| Everything, sealed enclosure | SD card surgery |

## The SMB games share

`/data/games` (and nothing else on /data) is exported as a guest
read-write SMB share for drag-drop ROM management. It is safe by
scoping, not masking: ssh keys, Wi-Fi credentials, and the box
profile are simply not inside the exported tree. The trust model is
identical to the web upload API - anyone on the LAN can add or
remove game files. The share must never be widened to /data (a
post-build assertion enforces this).

## Honest limits

- The everyday settings API is still unauthenticated on the LAN:
  a hostile network peer can rename the speaker or toggle pairing.
  Recoverable annoyances, accepted for now.
- No TLS: browsers may try https first and show a refused connection
  (type `http://` explicitly). Real TLS awaits a real multi-household
  deployment.
