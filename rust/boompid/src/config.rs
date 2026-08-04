//! Device configuration, loaded from TOML.
//!
//! On the appliance this lives at `/data/boompi.toml`: hardware facts are
//! seeded by the image build (per-box: INA260 bus, audio hints) and user
//! settings are written by the setup flow / Settings screen.

use anyhow::Context;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Speaker name (Bluetooth alias, shown on the Connect screen).
    pub name: String,
    /// Hardware model hint, e.g. "pi3" / "pi4".
    pub model: Option<String>,
    /// Battery monitor; omit entirely on boxes without an INA260.
    pub battery: Option<BatteryConfig>,
    /// Spotify Connect (librespot subprocess).
    pub spotify: SpotifyConfig,
    /// User settings (mutated at runtime, persisted back to disk).
    pub settings: SettingsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "Boompi".into(),
            model: None,
            battery: None,
            spotify: SpotifyConfig::default(),
            settings: SettingsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpotifyConfig {
    /// Spotify Connect (embedded librespot). On by default.
    pub enabled: bool,
}

impl Default for SpotifyConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BatteryConfig {
    /// Linux I2C bus number. The Pi 3 box reaches the INA260 through the
    /// HyperPixel overlay's bit-banged i2c-gpio bus, which is dynamically
    /// numbered `/dev/i2c-11` (v1 worked around this with an
    /// `ln -sf /dev/i2c-11 /dev/i2c-1` in kiosk.sh).
    /// TODO(Phase 1): optionally locate the adapter by name via
    /// /sys/class/i2c-adapter/*/name instead of a fixed number.
    pub i2c_bus: u8,
    /// 7-bit I2C address.
    pub address: u8,
    /// Pack voltage considered 0% (v1: 18.0).
    pub min_voltage: f32,
    /// Pack voltage considered 100% (v1: 24.98).
    pub max_voltage: f32,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            i2c_bus: 1,
            address: ina260_default_address(),
            min_voltage: 18.0,
            max_voltage: 24.98,
        }
    }
}

const fn ina260_default_address() -> u8 {
    0x40
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SettingsConfig {
    pub online_art_fallback: bool,
}

/// Load config from `path`, or defaults when `None`.
pub fn load(path: Option<&Path>) -> anyhow::Result<Config> {
    match path {
        None => Ok(Config::default()),
        Some(p) => {
            let raw = std::fs::read_to_string(p)
                .with_context(|| format!("reading config {}", p.display()))?;
            toml::from_str(&raw).with_context(|| format!("parsing config {}", p.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let cfg: Config = toml::from_str(
            r#"
            name = "Kitchen Boombox"
            model = "pi3"

            [battery]
            i2c_bus = 3
            address = 0x40
            min_voltage = 18.0
            max_voltage = 24.98

            [settings]
            online_art_fallback = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.name, "Kitchen Boombox");
        assert_eq!(cfg.battery.as_ref().unwrap().i2c_bus, 3);
        assert!(cfg.settings.online_art_fallback);
    }

    #[test]
    fn defaults_when_empty() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.name, "Boompi");
        assert!(cfg.battery.is_none());
        assert!(!cfg.settings.online_art_fallback);
    }
}
