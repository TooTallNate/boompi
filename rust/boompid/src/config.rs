//! Device configuration, loaded from TOML.
//!
//! On the appliance this lives at `/data/boompi.toml`: hardware facts are
//! seeded by the image build (per-box: INA260 bus, audio hints) and user
//! settings are written by the setup flow / Settings screen.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// AirPlay (shairport-sync child process).
    pub airplay: AirplayConfig,
    /// User settings (mutated at runtime, persisted back to disk).
    pub settings: SettingsConfig,
    /// First-boot setup finished? False → onboarding wizard + hotspot.
    /// The appliance image ships a config without this flag; the setup
    /// flow persists it. Dev boxes set it manually.
    pub setup_complete: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "Boompi".into(),
            model: None,
            battery: None,
            spotify: SpotifyConfig::default(),
            airplay: AirplayConfig::default(),
            settings: SettingsConfig::default(),
            setup_complete: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AirplayConfig {
    /// AirPlay receiver (shairport-sync, spawned by boompid). On by default;
    /// silently unavailable when shairport-sync is not installed.
    pub enabled: bool,
}

impl Default for AirplayConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SettingsConfig {
    pub online_art_fallback: bool,
    /// Panel UI theme ("dark" / "light").
    pub theme: boompi_proto::Theme,
    /// Advertised AirPlay device model ("" = shairport default).
    pub airplay_model: String,
}

/// Load config from `path`, falling back to a read-only `seed` when the
/// primary file doesn't exist yet.
///
/// The appliance splits config in two: `/etc/boompi/boompi.toml` (image-
/// baked hardware facts: model, battery bus) seeds the very first boot,
/// after which everything persists to `/data/boompi.toml` — which survives
/// OS reflashes.
pub fn load_with_seed(path: Option<&Path>, seed: Option<&Path>) -> anyhow::Result<Config> {
    if let Some(p) = path {
        if p.exists() {
            return load(Some(p));
        }
        if let Some(s) = seed {
            if s.exists() {
                tracing::info!(seed = %s.display(), target = %p.display(),
                    "primary config missing; seeding from image defaults");
                return load(Some(s));
            }
        }
    }
    load(path)
}

/// Load config from `path`, or defaults when `None`.
///
/// A missing file is not an error when the path was given explicitly: the
/// appliance points at `/data/boompi.toml` before first-boot setup has
/// written it.
pub fn load(path: Option<&Path>) -> anyhow::Result<Config> {
    match path {
        None => Ok(Config::default()),
        Some(p) => match std::fs::read_to_string(p) {
            Ok(raw) => {
                toml::from_str(&raw).with_context(|| format!("parsing config {}", p.display()))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %p.display(), "config not found; using defaults");
                Ok(Config::default())
            }
            Err(err) => {
                Err(err).with_context(|| format!("reading config {}", p.display()))
            }
        },
    }
}

/// Persist config to `path` atomically (write sibling temp file + rename).
pub fn save(cfg: &Config, path: &Path) -> anyhow::Result<()> {
    let toml = toml::to_string_pretty(cfg).context("serializing config")?;
    let toml = format!("# Boompi device configuration — managed by boompid.\n{toml}");
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &toml).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
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
        assert_eq!(cfg.settings.theme, boompi_proto::Theme::Dark);
    }

    #[test]
    fn save_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("boompi-cfg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boompi.toml");
        let mut cfg = Config::default();
        cfg.name = "Porch Box 📻".into();
        cfg.settings.theme = boompi_proto::Theme::Light;
        cfg.settings.online_art_fallback = true;
        save(&cfg, &path).unwrap();
        let loaded = load(Some(&path)).unwrap();
        assert_eq!(loaded.name, "Porch Box 📻");
        assert_eq!(loaded.settings.theme, boompi_proto::Theme::Light);
        assert!(loaded.settings.online_art_fallback);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_explicit_path_is_defaults() {
        let cfg = load(Some(Path::new("/nonexistent/boompi.toml"))).unwrap();
        assert_eq!(cfg.name, "Boompi");
    }

    #[test]
    fn seed_used_only_when_primary_missing() {
        let dir = std::env::temp_dir().join(format!("boompi-seed-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("data.toml");
        let seed = dir.join("seed.toml");
        std::fs::write(&seed, "name = \"Seeded\"\nmodel = \"pi3\"\n").unwrap();

        // Primary missing → seed wins.
        let cfg = load_with_seed(Some(&primary), Some(&seed)).unwrap();
        assert_eq!(cfg.name, "Seeded");
        assert_eq!(cfg.model.as_deref(), Some("pi3"));

        // Primary present → seed ignored.
        std::fs::write(&primary, "name = \"Configured\"\n").unwrap();
        let cfg = load_with_seed(Some(&primary), Some(&seed)).unwrap();
        assert_eq!(cfg.name, "Configured");

        // Neither exists → defaults.
        std::fs::remove_file(&primary).unwrap();
        std::fs::remove_file(&seed).unwrap();
        let cfg = load_with_seed(Some(&primary), Some(&seed)).unwrap();
        assert_eq!(cfg.name, "Boompi");
        std::fs::remove_dir_all(&dir).ok();
    }
}
