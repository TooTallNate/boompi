//! INA260 battery telemetry.
//!
//! Runs on a dedicated thread (I2C reads are blocking). The INA260 is
//! sampled every second to feed the SoC estimator (coulomb counting
//! needs continuous integration); broadcasts keep the old cadence -
//! 30 s normally, 1 s while any client fast-polls (battery panel
//! open). Absent or unresponsive hardware disables the feature
//! gracefully - not every box has one.
//!
//! Learned calibration (full voltage, capacity) and the coulomb anchor
//! persist in /data so they survive restarts and OTA updates.

#![cfg(target_os = "linux")]

use crate::config::BatteryConfig;
use crate::soc::{
    Calibration, Safeguard, SafeguardAction, SafeguardParams, Snapshot, SocEstimator, SocParams,
};
use crate::state::{now_ms, SharedApp};
use boompi_proto::{Battery, ServerMessage};
use std::time::{Duration, Instant};

/// Grace period between the poweroff broadcast (panel notice) and the
/// actual shutdown.
const POWEROFF_GRACE_SECS: u32 = 20;

/// Charging when current into the pack exceeds 20 mA (v1 rule).
const CHARGING_THRESHOLD_A: f64 = -0.020;

const CAL_PATH: &str = "/data/boompi-battery.json";
/// Snapshot the coulomb anchor this often (also written on calibration
/// changes).
const PERSIST_INTERVAL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Persisted {
    calibration: Calibration,
    snapshot: Option<Snapshot>,
}

fn load_persisted() -> Persisted {
    match std::fs::read(CAL_PATH) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(%err, "unparseable {CAL_PATH}; starting fresh");
                Persisted::default()
            }
        },
        Err(_) => Persisted::default(),
    }
}

fn save_persisted(p: &Persisted) {
    let tmp = format!("{CAL_PATH}.tmp");
    let write = || -> std::io::Result<()> {
        std::fs::write(&tmp, serde_json::to_vec_pretty(p).expect("serialize"))?;
        std::fs::rename(&tmp, CAL_PATH)
    };
    if let Err(err) = write() {
        tracing::warn!(%err, "failed to persist battery calibration");
    }
}

pub fn spawn(app: SharedApp) {
    let Some(cfg) = app.cfg.battery.clone() else {
        tracing::info!("no [battery] config; INA260 telemetry disabled");
        return;
    };
    std::thread::Builder::new()
        .name("battery".into())
        .spawn(move || run(app, cfg))
        .expect("spawn battery thread");
}

fn run(app: SharedApp, cfg: BatteryConfig) {
    let path = format!("/dev/i2c-{}", cfg.i2c_bus);
    let dev = match linux_embedded_hal::I2cdev::new(&path) {
        Ok(dev) => dev,
        Err(err) => {
            tracing::error!(%err, %path, "cannot open I2C bus; battery telemetry disabled");
            return;
        }
    };
    let mut ina = ina260::Ina260::new(dev, cfg.address);

    match ina.manufacturer_id() {
        Ok(ina260::MANUFACTURER_ID) => {}
        Ok(other) => tracing::warn!(
            "unexpected INA260 manufacturer id {other:#06x} on {path} @ {:#04x}",
            cfg.address
        ),
        Err(err) => {
            tracing::error!(%err, %path, "INA260 not responding; battery telemetry disabled");
            return;
        }
    }
    if let Err(err) = ina.write_config(ina260::V1_CONFIG) {
        tracing::warn!(%err, "failed to write INA260 config; continuing with defaults");
    }
    tracing::info!(%path, address = format!("{:#04x}", cfg.address), "INA260 battery telemetry active");

    let persisted = load_persisted();
    let mut estimator = SocEstimator::new(
        SocParams {
            min_voltage: cfg.min_voltage,
            default_full_voltage: cfg.max_voltage,
        },
        persisted.calibration.clone(),
    );
    if let Some(snap) = &persisted.snapshot {
        if let Ok((v, _, _)) = read_ina(&mut ina) {
            estimator.restore(snap, v as f32);
        }
    }
    if let Some(fv) = estimator.calibration().full_voltage {
        tracing::info!(
            full_voltage = fv,
            capacity_ah = estimator.calibration().capacity_ah,
            "battery calibration loaded"
        );
    }

    let mut safeguard = Safeguard::new(SafeguardParams {
        warn_soc: cfg.warn_soc,
        warn_clear_soc: cfg.warn_soc + 0.05,
        shutdown_soc: if cfg.shutdown_soc > 0.0 {
            cfg.shutdown_soc
        } else {
            -1.0
        },
        shutdown_voltage: cfg.shutdown_voltage,
        sustain_secs: 60.0,
    });
    let mut was_low = false;
    let mut ticks_until_broadcast = 0u32;
    let mut last_sample = Instant::now();
    let mut last_persist = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let (voltage, current, power) = match read_ina(&mut ina) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(%err, "battery read failed");
                last_sample = Instant::now(); // do not integrate the gap
                continue;
            }
        };
        let dt = last_sample.elapsed().as_secs_f32();
        last_sample = Instant::now();
        estimator.update(voltage as f32, current as f32, dt);

        if estimator.take_dirty() || last_persist.elapsed() >= PERSIST_INTERVAL {
            last_persist = Instant::now();
            save_persisted(&Persisted {
                calibration: estimator.calibration().clone(),
                snapshot: Some(estimator.snapshot()),
            });
        }

        let charging = current <= CHARGING_THRESHOLD_A;
        let action = safeguard.update(estimator.soc(), voltage as f32, charging, dt);
        if action == SafeguardAction::PowerOff && cfg.auto_shutdown {
            power_off(&app, voltage as f32, estimator.soc());
        }

        let fast = app.shared.blocking_read().fast_poll_clients > 0;
        let low_edge = safeguard.low() != was_low;
        was_low = safeguard.low();
        if ticks_until_broadcast == 0 || fast || low_edge {
            ticks_until_broadcast = 30; // slow cadence: 30 s
            let battery = Battery {
                voltage: voltage as f32,
                current: current as f32,
                power: power as f32,
                percentage: estimator.soc(),
                charging,
                full: estimator.full(),
                low: safeguard.low(),
                time_remaining_secs: estimator.time_remaining_secs(),
                ts: now_ms(),
            };
            app.shared.blocking_write().battery = Some(battery.clone());
            app.broadcast(ServerMessage::Battery(battery));
        }
        ticks_until_broadcast -= 1;
    }
}

/// Battery empty: broadcast the notice (panel shows it during the
/// grace period), then orderly poweroff. The GPU-wedge incident taught
/// us orderly shutdown can hang in stop jobs, so a forced poweroff
/// backstops it - at this point the alternative is draining into the
/// BMS cutoff anyway.
fn power_off(app: &SharedApp, voltage: f32, soc: f32) {
    tracing::error!(voltage, soc, "battery empty - powering off");
    app.broadcast(ServerMessage::PowerOff {
        reason: "Battery empty".into(),
        in_secs: POWEROFF_GRACE_SECS,
    });
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(POWEROFF_GRACE_SECS as u64));
        let _ = std::process::Command::new("sh")
            .args(["-c", "systemctl poweroff; sleep 90; poweroff -f"])
            .spawn();
    });
}

/// One INA260 read: (volts, amps, watts).
fn read_ina(
    ina: &mut ina260::Ina260<linux_embedded_hal::I2cdev>,
) -> anyhow::Result<(f64, f64, f64)> {
    let voltage = ina.voltage().map_err(|e| anyhow::anyhow!("voltage: {e}"))?;
    let current = ina.current().map_err(|e| anyhow::anyhow!("current: {e}"))?;
    let power = ina.power().map_err(|e| anyhow::anyhow!("power: {e}"))?;
    Ok((voltage, current, power))
}
