//! INA260 battery telemetry.
//!
//! Runs on a dedicated thread (I2C reads are blocking). Poll cadence
//! matches v1: 30 s normally, 1 s while any client fast-polls (battery
//! panel open). Absent or unresponsive hardware disables the feature
//! gracefully — the Pi 4 box may not have one.

#![cfg(target_os = "linux")]

use crate::config::BatteryConfig;
use crate::state::{now_ms, SharedApp};
use boompi_proto::{Battery, ServerMessage};
use std::time::Duration;

/// Charging when current into the pack exceeds 20 mA (v1 rule).
const CHARGING_THRESHOLD_A: f64 = -0.020;

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

    let mut ticks_until_poll = 0u32;
    loop {
        let fast = app.shared.blocking_read().fast_poll_clients > 0;
        if ticks_until_poll == 0 || fast {
            match read_battery(&mut ina, &cfg) {
                Ok(battery) => {
                    app.shared.blocking_write().battery = Some(battery.clone());
                    app.broadcast(ServerMessage::Battery(battery));
                }
                Err(err) => tracing::warn!(%err, "battery read failed"),
            }
            ticks_until_poll = 30; // slow cadence: one poll per 30 ticks (30 s)
        }
        ticks_until_poll -= 1;
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn read_battery(
    ina: &mut ina260::Ina260<linux_embedded_hal::I2cdev>,
    cfg: &BatteryConfig,
) -> anyhow::Result<Battery> {
    let voltage = ina.voltage().map_err(|e| anyhow::anyhow!("voltage: {e}"))?;
    let current = ina.current().map_err(|e| anyhow::anyhow!("current: {e}"))?;
    let power = ina.power().map_err(|e| anyhow::anyhow!("power: {e}"))?;
    let percentage =
        ((voltage as f32 - cfg.min_voltage) / (cfg.max_voltage - cfg.min_voltage)).clamp(0.0, 1.0);
    Ok(Battery {
        voltage: voltage as f32,
        current: current as f32,
        power: power as f32,
        percentage,
        charging: current <= CHARGING_THRESHOLD_A,
        ts: now_ms(),
    })
}
