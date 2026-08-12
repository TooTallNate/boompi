---
"boompi": minor
---

Battery intelligence: a proper state-of-charge estimator replaces the
static voltage map. Full charge is detected the way chargers define it
(sustained current tapering to zero at a voltage plateau), and the
plateau voltage is learned and persisted per box - so every box
self-calibrates to its own CC/CV converter setpoint, including after
the setpoint is changed. Once a full charge anchors the estimator, SoC
is coulomb-counted from the INA260 (immune to load sag), pack capacity
is learned from ordinary partial discharges, and a time-remaining
estimate appears while discharging. New in Home Assistant: battery
time remaining and charging entities. The panel battery screen gains a
TIME LEFT stat and a full badge, and the web settings UI now shows
battery status.
