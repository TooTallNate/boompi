---
"boompi": patch
---

Retire kexec update trials: kexec into a different kernel build hangs
after "Bye!" on both boards (long known on the Pi 3, confirmed on the
Pi 4 during the v2.0.0 rollout). The Pi 3 keeps its one-shot PM_RSTS
firmware trial; the Pi 4, whose rev <= 1.3 PMIC power-cycle wipes
every firmware one-shot flag, now commits the candidate before the
reboot and rolls back automatically if it boots unhealthy.
