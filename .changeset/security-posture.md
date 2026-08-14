---
"boompi": minor
---

Security posture rework. SSH is key-only (PasswordAuthentication no;
the root password works exclusively on the HDMI console and is
documented as such) and the image ships trusting nobody: the baked
authorized_keys is gone, per-box keys live at /data/ssh/ and arrive
via the flash-time bundle, the web hardware page, the provision
scripts, or `boompi-box add-key`. The hardware page/API can be
locked - refused unless an ssh key is authorized first, so the lock
can never remove the last remote path in - after which boot
configuration is ssh-only via the new `boompi-box` CLI (show, edit,
apply, lock/unlock, add-key, export - the provisioning-bundle
convenience works on locked boxes). Factory reset is removed from
the web UI and network APIs entirely: `boompi-factory-reset` over
ssh or console. Recovery matrix and the full story in
docs/SECURITY.md.
