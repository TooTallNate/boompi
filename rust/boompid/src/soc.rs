//! Battery state-of-charge estimation.
//!
//! The naive approach (linear map of pack voltage between configured
//! min/max) breaks down in practice: every box's CC/CV converter is set
//! to a slightly different full voltage, voltage sags under load, and
//! rises while charging. This module replaces it with a small estimator
//! fed one `(voltage, current, dt)` sample at a time:
//!
//! - **Full detection**: "full" is defined the way real chargers define
//!   it - a sustained charge current that tapers to ~zero while the
//!   voltage plateaus. The plateau voltage is *learned* and persisted,
//!   so each box self-calibrates to its own converter setpoint. Raising
//!   the setpoint is adopted immediately (a higher plateau can only mean
//!   the converter goes higher); lowering is adopted only after several
//!   consistent full events, so one anomalous session cannot corrupt
//!   the calibration.
//! - **Coulomb counting**: the INA260 measures pack current directly,
//!   so once a full event anchors the counter, SoC is integrated charge
//!   rather than instantaneous voltage - immune to load sag. A slow
//!   voltage-based correction at rest bounds integration drift.
//! - **Capacity learning**: pairing integrated Ah-out with the resting
//!   voltage estimate lets us learn the pack's effective capacity from
//!   ordinary partial discharges, which unlocks time-remaining.
//!
//! Sign convention matches the INA260 wiring: current > 0 discharging,
//! current < 0 charging. Platform-independent so it unit-tests on any
//! host; the linux-only battery thread owns I/O and persistence.

use serde::{Deserialize, Serialize};

/// Sustained current below this (i.e. more negative) counts as a real
/// charging session, not float trickle or sensor noise.
const CHARGE_ACTIVE_A: f32 = -0.15;
/// |current| below this is the "terminated / float" band.
const FULL_BAND_A: f32 = 0.08;
/// Sustained current above this is a real discharge.
const DISCHARGE_ACTIVE_A: f32 = 0.08;
/// Minimum time spent actively charging before a taper can count as a
/// full event (rejects blips and charger-restart churn).
const MIN_CHARGE_SECS: f32 = 180.0;
/// Time the current must hold inside the full band, at plateau voltage,
/// before we declare full after a charge.
const FULL_HOLD_SECS: f32 = 180.0;
/// Time resting at the learned full voltage before we declare full
/// without having watched the taper (e.g. boompid restarted on float).
const REST_FULL_HOLD_SECS: f32 = 300.0;
/// Voltage must stay within this of the session's peak to count as a
/// plateau (an unplug shows a sag at the same moment the current dies).
const PLATEAU_DELTA_V: f32 = 0.08;
/// Resting-entry margin below the learned full voltage.
const REST_FULL_MARGIN_V: f32 = 0.10;
/// A plateau this far above the learned value re-learns immediately.
const LEARN_UP_DELTA_V: f32 = 0.05;
/// A plateau this far below the learned value is suspicious; require
/// `LEARN_DOWN_EVENTS` consistent full events before adopting it.
const LEARN_DOWN_DELTA_V: f32 = 0.10;
const LEARN_DOWN_EVENTS: u8 = 3;
/// EWMA time constants (seconds).
const TAU_PHASE_SECS: f32 = 10.0;
const TAU_DRAW_SECS: f32 = 300.0;
/// Drift correction toward the voltage estimate while resting.
const TAU_REST_CORRECT_SECS: f32 = 1800.0;
/// Capacity learning: minimum discharge depth before an estimate is
/// meaningful, and sanity bounds on the result.
const CAP_LEARN_MIN_AH: f32 = 0.8;
const CAP_LEARN_MAX_VSOC: f32 = 0.80;
const CAP_MIN_AH: f32 = 1.0;
const CAP_MAX_AH: f32 = 50.0;
const TAU_CAP_LEARN_SECS: f32 = 3600.0;

/// Boot-time facts from config.
#[derive(Debug, Clone, Copy)]
pub struct SocParams {
    /// Pack voltage considered 0%.
    pub min_voltage: f32,
    /// Fallback full voltage until one is learned.
    pub default_full_voltage: f32,
}

/// Learned, persisted calibration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {
    /// CV plateau voltage observed at the last accepted full event.
    pub full_voltage: Option<f32>,
    /// Effective pack capacity in Ah.
    pub capacity_ah: Option<f32>,
    /// Candidate lower full voltage: (voltage, consecutive events).
    pub pending_lower: Option<(f32, u8)>,
}

/// Runtime snapshot so a boompid restart doesn't lose the coulomb
/// anchor. Restored only if the pack voltage hasn't moved meaningfully
/// while we were away.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub ah_out: f64,
    pub anchored: bool,
    pub voltage: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Rest,
    Charging,
    Full,
    Discharging,
}

pub struct SocEstimator {
    params: SocParams,
    cal: Calibration,
    dirty: bool,

    phase: Phase,
    /// Short EWMA of current for phase decisions.
    i_phase: f32,
    /// Long EWMA of discharge current for time-remaining.
    i_draw: f32,
    /// Latest raw sample.
    voltage: f32,
    /// Seconds spent actively charging in the current session.
    charging_secs: f32,
    /// Peak voltage seen in the current charge session.
    session_max_v: f32,
    /// Seconds the current has held inside the full band.
    band_secs: f32,
    /// Coulomb counter: Ah out of the pack since the last full anchor.
    ah_out: f64,
    anchored: bool,
    seeded: bool,
}

impl SocEstimator {
    pub fn new(params: SocParams, cal: Calibration) -> Self {
        Self {
            params,
            cal,
            dirty: false,
            phase: Phase::Rest,
            i_phase: 0.0,
            i_draw: 0.0,
            voltage: 0.0,
            charging_secs: 0.0,
            session_max_v: 0.0,
            band_secs: 0.0,
            ah_out: 0.0,
            anchored: false,
            seeded: false,
        }
    }

    /// Restore the coulomb anchor from a persisted snapshot, if the
    /// pack hasn't moved since it was taken.
    pub fn restore(&mut self, snap: &Snapshot, current_voltage: f32) {
        if snap.anchored && (snap.voltage - current_voltage).abs() < 0.10 {
            self.ah_out = snap.ah_out;
            self.anchored = true;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            ah_out: self.ah_out,
            anchored: self.anchored,
            voltage: self.voltage,
        }
    }

    pub fn calibration(&self) -> &Calibration {
        &self.cal
    }

    /// True once since the last call if the calibration changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn effective_full_voltage(&self) -> f32 {
        self.cal
            .full_voltage
            .unwrap_or(self.params.default_full_voltage)
    }

    /// Linear voltage map against the learned (or default) full voltage.
    fn voltage_soc(&self, v: f32) -> f32 {
        let full = self.effective_full_voltage();
        ((v - self.params.min_voltage) / (full - self.params.min_voltage)).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, voltage: f32, current: f32, dt_secs: f32) {
        if dt_secs <= 0.0 {
            return;
        }
        self.voltage = voltage;
        if !self.seeded {
            self.i_phase = current;
            self.seeded = true;
        }
        let a_phase = (dt_secs / TAU_PHASE_SECS).min(1.0);
        self.i_phase += (current - self.i_phase) * a_phase;
        if current > 0.0 {
            if self.i_draw <= 0.0 {
                self.i_draw = current; // seed: avoid inflated early estimates
            }
            let a_draw = (dt_secs / TAU_DRAW_SECS).min(1.0);
            self.i_draw += (current - self.i_draw) * a_draw;
        }

        // Coulomb integration (charge in reduces ah_out, floor at 0:
        // charge past the anchor just means we're approaching full).
        if self.anchored {
            self.ah_out = (self.ah_out + current as f64 * dt_secs as f64 / 3600.0).max(0.0);
        }

        let i = self.i_phase;
        if i < CHARGE_ACTIVE_A {
            if self.phase != Phase::Charging {
                self.session_max_v = voltage;
            }
            self.phase = Phase::Charging;
            self.charging_secs += dt_secs;
            self.session_max_v = self.session_max_v.max(voltage);
            self.band_secs = 0.0;
        } else if i.abs() < FULL_BAND_A {
            match self.phase {
                Phase::Full => {}
                Phase::Charging if self.charging_secs >= MIN_CHARGE_SECS => {
                    // Taper: only counts while the voltage holds the
                    // session plateau (an unplug sags immediately).
                    if voltage >= self.session_max_v - PLATEAU_DELTA_V {
                        self.band_secs += dt_secs;
                        if self.band_secs >= FULL_HOLD_SECS {
                            self.full_event(voltage);
                        }
                    } else {
                        self.band_secs = 0.0;
                    }
                }
                _ => {
                    // Resting at the learned full voltage (e.g. we
                    // restarted while on the float charger).
                    if voltage >= self.effective_full_voltage() - REST_FULL_MARGIN_V {
                        self.band_secs += dt_secs;
                        if self.band_secs >= REST_FULL_HOLD_SECS {
                            self.enter_full();
                        }
                    } else {
                        self.band_secs = 0.0;
                        // Rest-phase drift correction: converge the
                        // coulomb counter toward the voltage estimate.
                        if self.anchored {
                            if let Some(cap) = self.cal.capacity_ah {
                                let target = (1.0 - self.voltage_soc(voltage)) as f64 * cap as f64;
                                let a = (dt_secs / TAU_REST_CORRECT_SECS).min(1.0) as f64;
                                self.ah_out += (target - self.ah_out) * a;
                            }
                        }
                        if self.phase == Phase::Charging {
                            self.phase = Phase::Rest;
                            self.charging_secs = 0.0;
                        }
                    }
                }
            }
        } else if i > DISCHARGE_ACTIVE_A {
            if self.phase != Phase::Discharging {
                self.phase = Phase::Discharging;
                self.charging_secs = 0.0;
                self.band_secs = 0.0;
            }
            self.learn_capacity(voltage, dt_secs);
        }
        // Currents between the bands (light trickle) keep the phase.
    }

    fn enter_full(&mut self) {
        self.phase = Phase::Full;
        self.anchored = true;
        self.ah_out = 0.0;
        self.band_secs = 0.0;
        self.charging_secs = 0.0;
    }

    /// A watched charge taper completed at `plateau_v`: learn from it,
    /// then anchor.
    fn full_event(&mut self, plateau_v: f32) {
        match self.cal.full_voltage {
            None => {
                self.cal.full_voltage = Some(plateau_v);
                self.cal.pending_lower = None;
                self.dirty = true;
            }
            Some(learned) if plateau_v > learned + LEARN_UP_DELTA_V => {
                // The converter can only reach a higher plateau if its
                // setpoint was raised: adopt immediately.
                self.cal.full_voltage = Some(plateau_v);
                self.cal.pending_lower = None;
                self.dirty = true;
            }
            Some(learned) if plateau_v < learned - LEARN_DOWN_DELTA_V => {
                // Lower than we believe full is: could be a lowered
                // setpoint, could be one bad session. Adopt only after
                // several consistent events.
                let (v, n) = match self.cal.pending_lower {
                    Some((v, n)) if (v - plateau_v).abs() < LEARN_DOWN_DELTA_V => {
                        (v.min(plateau_v), n + 1)
                    }
                    _ => (plateau_v, 1),
                };
                if n >= LEARN_DOWN_EVENTS {
                    self.cal.full_voltage = Some(plateau_v);
                    self.cal.pending_lower = None;
                } else {
                    self.cal.pending_lower = Some((v, n));
                }
                self.dirty = true;
            }
            Some(learned) => {
                // Close to the learned value: track slow drift.
                let blended = learned * 0.9 + plateau_v * 0.1;
                if (blended - learned).abs() > 0.005 {
                    self.cal.full_voltage = Some(blended);
                    self.dirty = true;
                }
                self.cal.pending_lower = None;
            }
        }
        self.enter_full();
    }

    /// While discharging with a coulomb anchor, pair integrated Ah with
    /// the voltage estimate to learn effective capacity.
    fn learn_capacity(&mut self, voltage: f32, dt_secs: f32) {
        if !self.anchored || self.ah_out < CAP_LEARN_MIN_AH as f64 {
            return;
        }
        let v_soc = self.voltage_soc(voltage);
        if v_soc > CAP_LEARN_MAX_VSOC {
            return;
        }
        let est = (self.ah_out / (1.0 - v_soc) as f64) as f32;
        if !(CAP_MIN_AH..=CAP_MAX_AH).contains(&est) {
            return;
        }
        match self.cal.capacity_ah {
            None => {
                self.cal.capacity_ah = Some(est);
                self.dirty = true;
            }
            Some(cap) => {
                let a = (dt_secs / TAU_CAP_LEARN_SECS).min(1.0);
                let blended = cap + (est - cap) * a;
                if (blended - cap).abs() > 0.01 {
                    self.cal.capacity_ah = Some(blended);
                    self.dirty = true;
                }
            }
        }
    }

    pub fn full(&self) -> bool {
        self.phase == Phase::Full
    }

    /// State of charge, 0.0-1.0. Exactly 1.0 only when full is
    /// detected; capped just below while a charge is in flight (the
    /// elevated CV voltage would otherwise read 100% early).
    pub fn soc(&self) -> f32 {
        if self.phase == Phase::Full {
            return 1.0;
        }
        let soc = match (self.anchored, self.cal.capacity_ah) {
            (true, Some(cap)) if cap > 0.0 => {
                (1.0 - (self.ah_out / cap as f64) as f32).clamp(0.0, 1.0)
            }
            _ => self.voltage_soc(self.voltage),
        };
        if self.phase == Phase::Charging {
            soc.min(0.99)
        } else {
            soc
        }
    }

    /// Estimated time to empty, while discharging with a learned
    /// capacity. `None` while charging/full/resting or before the pack
    /// has taught us enough.
    pub fn time_remaining_secs(&self) -> Option<u32> {
        if self.phase != Phase::Discharging || self.i_draw < 0.05 {
            return None;
        }
        let cap = self.cal.capacity_ah?;
        let remaining_ah = self.soc() * cap;
        let secs = remaining_ah / self.i_draw * 3600.0;
        if secs.is_finite() && secs >= 0.0 {
            Some(secs.min(99.0 * 3600.0) as u32)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMS: SocParams = SocParams {
        min_voltage: 18.0,
        default_full_voltage: 24.98,
    };

    fn estimator(cal: Calibration) -> SocEstimator {
        SocEstimator::new(PARAMS, cal)
    }

    /// Feed `secs` of a steady sample at 1 Hz.
    fn feed(e: &mut SocEstimator, v: f32, i: f32, secs: u32) {
        for _ in 0..secs {
            e.update(v, i, 1.0);
        }
    }

    /// A realistic CC/CV charge ending at `plateau_v`.
    fn charge_to_full(e: &mut SocEstimator, plateau_v: f32) {
        feed(e, plateau_v - 0.6, -2.0, 600); // bulk
        feed(e, plateau_v - 0.2, -0.8, 300); // taper
        feed(e, plateau_v, -0.3, 300); // late taper
        feed(e, plateau_v, -0.03, 400); // terminated, float
    }

    #[test]
    fn learns_full_voltage_from_taper() {
        let mut e = estimator(Calibration::default());
        charge_to_full(&mut e, 24.18);
        assert!(e.full());
        assert_eq!(e.soc(), 1.0);
        let learned = e.calibration().full_voltage.unwrap();
        assert!((learned - 24.18).abs() < 0.01, "learned {learned}");
        assert!(e.take_dirty());
    }

    #[test]
    fn pot_raised_adopts_higher_plateau_immediately() {
        let mut e = estimator(Calibration {
            full_voltage: Some(24.18),
            ..Default::default()
        });
        charge_to_full(&mut e, 24.85);
        let learned = e.calibration().full_voltage.unwrap();
        assert!((learned - 24.85).abs() < 0.01, "learned {learned}");
    }

    #[test]
    fn while_charging_above_learned_full_soc_is_capped_not_full() {
        // Pot was just raised: pack is at the old "full" voltage but
        // still drawing heavy charge current. Must not report full.
        let mut e = estimator(Calibration {
            full_voltage: Some(24.18),
            ..Default::default()
        });
        feed(&mut e, 24.30, -1.5, 600);
        assert!(!e.full());
        assert!(e.soc() <= 0.99);
    }

    #[test]
    fn lower_plateau_needs_repeated_confirmation() {
        let mut e = estimator(Calibration {
            full_voltage: Some(24.85),
            ..Default::default()
        });
        // Two full events at a lower plateau: not adopted yet.
        for _ in 0..2 {
            charge_to_full(&mut e, 24.20);
            feed(&mut e, 24.0, 0.4, 600); // discharge in between
        }
        assert!((e.calibration().full_voltage.unwrap() - 24.85).abs() < 0.01);
        // Third consistent event adopts it.
        charge_to_full(&mut e, 24.20);
        assert!((e.calibration().full_voltage.unwrap() - 24.20).abs() < 0.01);
    }

    #[test]
    fn weak_charger_cycling_never_reads_full() {
        // The 60W brick pattern: charge bursts alternating with the
        // load pulling from the battery, current never resting near 0.
        let mut e = estimator(Calibration::default());
        for _ in 0..60 {
            feed(&mut e, 24.2, -1.0, 40);
            feed(&mut e, 24.0, 0.45, 60);
        }
        assert!(!e.full());
        assert!(e.calibration().full_voltage.is_none());
    }

    #[test]
    fn unplug_during_taper_is_not_full() {
        let mut e = estimator(Calibration::default());
        feed(&mut e, 24.4, -2.0, 600);
        feed(&mut e, 24.8, -0.5, 300);
        // Unplugged: voltage sags off the plateau as current dies, then
        // the load discharges.
        feed(&mut e, 24.35, 0.0, 30);
        feed(&mut e, 24.3, 0.4, 600);
        assert!(!e.full());
        assert!(e.calibration().full_voltage.is_none());
    }

    #[test]
    fn resting_on_float_after_restart_reaches_full() {
        // boompid restarts while the box sits on the float charger.
        let mut e = estimator(Calibration {
            full_voltage: Some(24.84),
            ..Default::default()
        });
        feed(&mut e, 24.82, 0.01, REST_FULL_HOLD_SECS as u32 + 30);
        assert!(e.full());
        assert_eq!(e.soc(), 1.0);
    }

    #[test]
    fn coulomb_soc_and_capacity_learning() {
        let mut e = estimator(Calibration {
            full_voltage: Some(24.84),
            ..Default::default()
        });
        charge_to_full(&mut e, 24.84);
        // Discharge 2.0 Ah at 0.4 A (5 h) while the voltage sinks to
        // the ~50% region of the map.
        let hours = 5.0;
        let steps = (hours * 3600.0) as u32;
        for s in 0..steps {
            let frac = s as f32 / steps as f32;
            let v = 24.6 - frac * 3.2; // ends ~21.4 V
            e.update(v, 0.4, 1.0);
        }
        let cap = e.calibration().capacity_ah.expect("capacity learned");
        // 2 Ah for ~half the pack: expect ~4-5 Ah learned.
        assert!((3.0..7.0).contains(&cap), "capacity {cap}");
        let soc = e.soc();
        assert!((0.35..0.65).contains(&soc), "soc {soc}");
        assert!(e.time_remaining_secs().is_some());
        let secs = e.time_remaining_secs().unwrap();
        // Remaining ~half pack at 0.4 A: hours, not minutes.
        assert!((3600..12 * 3600).contains(&secs), "time {secs}");
    }

    #[test]
    fn no_time_remaining_without_capacity_or_while_charging() {
        let mut e = estimator(Calibration::default());
        feed(&mut e, 23.0, 0.4, 300);
        assert_eq!(e.time_remaining_secs(), None); // no capacity yet
        let mut e = estimator(Calibration {
            full_voltage: Some(24.84),
            capacity_ah: Some(4.0),
            ..Default::default()
        });
        feed(&mut e, 24.0, -1.0, 300);
        assert_eq!(e.time_remaining_secs(), None); // charging
    }

    #[test]
    fn snapshot_restores_only_when_voltage_matches() {
        let mut e = estimator(Calibration {
            full_voltage: Some(24.84),
            capacity_ah: Some(4.0),
            ..Default::default()
        });
        charge_to_full(&mut e, 24.84);
        feed(&mut e, 23.5, 0.4, 3600); // 0.4 Ah out
        let snap = e.snapshot();

        let mut fresh = estimator(e.calibration().clone());
        fresh.restore(&snap, 23.52); // same voltage: resume anchor
        feed(&mut fresh, 23.5, 0.4, 60);
        assert!((fresh.soc() - e.soc()).abs() < 0.02);

        let mut moved = estimator(e.calibration().clone());
        moved.restore(&snap, 22.8); // pack moved while we were away
        feed(&mut moved, 22.8, 0.4, 60);
        // Falls back to the voltage map (not the stale anchor).
        let expect = (22.8 - 18.0) / (24.84 - 18.0);
        assert!((moved.soc() - expect).abs() < 0.05);
    }

    #[test]
    fn voltage_map_uses_learned_full() {
        let mut e = estimator(Calibration {
            full_voltage: Some(24.2),
            ..Default::default()
        });
        // At the learned full voltage the map reads ~100% even though
        // it is far below the config default of 24.98.
        e.update(24.2, 0.0, 1.0);
        assert!(e.soc() > 0.99);
    }
}
