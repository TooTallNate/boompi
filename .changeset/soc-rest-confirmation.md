---
"boompi": patch
---

The battery estimator no longer gets stuck below 100% when the
charger's top voltage drops. It already noticed the lower plateau and
waited for several full charge cycles to confirm it (guarding against
one anomalous session) - but a box that lives on its charger may not
produce cycles for weeks, so the display sat at 88% indefinitely.
Resting pinned at the candidate plateau with no current flowing now
counts as confirmation too: about half a day of quiet sitting adopts
the new full voltage and re-baselines the gauge. The old behavior
remains for boxes that do cycle - and if your charge voltage dropped
without you touching the charger, do check the charger and the pack's
cell balance; the gauge adapting doesn't make the electrons come back.
