//! Device configuration, loaded from TOML.
//!
//! On the appliance this lives at `/data/boompi.toml`: hardware facts are
//! seeded by the image build (per-box: INA260 bus, audio hints) and user
//! settings are written by the setup flow / Settings screen.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Speaker name (Bluetooth alias, shown on the Connect screen).
    pub name: String,
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
    /// Per-device Bluetooth volume-mode assignments (address → mode);
    /// devices absent from the map are `Auto`.
    pub bt_volume_modes: std::collections::HashMap<String, boompi_proto::BtVolumeMode>,
    /// User timezone (IANA name). The system copy lives in /etc/localtime
    /// on the rootfs, which an A/B update replaces wholesale - this is
    /// the durable copy, re-applied at startup.
    pub timezone: Option<String>,
    /// NTP on/off, if the user ever toggled it (None = image default).
    pub ntp: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "Boompi".into(),
            battery: None,
            spotify: SpotifyConfig::default(),
            airplay: AirplayConfig::default(),
            settings: SettingsConfig::default(),
            setup_complete: false,
            bt_volume_modes: Default::default(),
            timezone: None,
            ntp: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
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
    /// State of charge at or below which the panel shows the
    /// low-battery warning (clears at +5 points or on charge).
    pub warn_soc: f32,
    /// Automatic shutdown: SoC floor. 0 disables SoC-based shutdown.
    pub shutdown_soc: f32,
    /// Automatic shutdown: sustained pack-voltage floor. 0 disables
    /// voltage-based shutdown.
    pub shutdown_voltage: f32,
    /// Set false to disable the automatic shutdown entirely (the
    /// warning banner still shows).
    pub auto_shutdown: bool,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            i2c_bus: 1,
            address: ina260_default_address(),
            min_voltage: 18.0,
            max_voltage: 24.98,
            warn_soc: 0.15,
            shutdown_soc: 0.05,
            shutdown_voltage: 18.3,
            auto_shutdown: true,
        }
    }
}

const fn ina260_default_address() -> u8 {
    0x40
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsConfig {
    pub online_art_fallback: bool,
    /// Panel UI theme ("dark" / "light").
    pub theme: boompi_proto::Theme,
    /// Advertised AirPlay device model ("" = shairport default).
    pub airplay_model: String,
    /// Panel UI scale (1.0 = design size); per-board seeds may ship
    /// larger defaults for small high-DPI panels.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// Active emoji font (catalog id in fonts.rs; "noto" ships with
    /// the image, others download to /data/fonts).
    #[serde(default = "default_emoji_font")]
    pub emoji_font: String,
    /// Which releases the software updater follows ("stable" /
    /// "edge").
    #[serde(default)]
    pub update_channel: boompi_proto::UpdateChannel,
    /// Classic-AirPlay-only receiver mode (working speaker-side
    /// transport controls, no AirPlay 2 / multi-room).
    #[serde(default)]
    pub airplay_classic: bool,
    /// 24-hour clock on the panel (12-hour with AM/PM when false).
    #[serde(default)]
    pub clock_24h: bool,
    /// MQTT broker ("host" or "host:port"; empty = disabled) +
    /// credentials for the Home Assistant integration.
    #[serde(default)]
    pub mqtt_broker: String,
    #[serde(default)]
    pub mqtt_username: String,
    #[serde(default)]
    pub mqtt_password: String,
    /// Idle screensaver style ("off" / "clock" / "matrix" / "art").
    #[serde(default)]
    pub screensaver: boompi_proto::ScreensaverKind,
    /// Idle minutes before the screensaver starts.
    #[serde(default = "default_screensaver_min")]
    pub screensaver_min: u32,
}

fn default_screensaver_min() -> u32 {
    10
}

fn default_emoji_font() -> String {
    "noto".into()
}

fn default_ui_scale() -> f32 {
    1.0
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            online_art_fallback: false,
            theme: boompi_proto::Theme::default(),
            airplay_model: String::new(),
            ui_scale: default_ui_scale(),
            emoji_font: default_emoji_font(),
            update_channel: boompi_proto::UpdateChannel::default(),
            airplay_classic: false,
            clock_24h: false,
            mqtt_broker: String::new(),
            mqtt_username: String::new(),
            mqtt_password: String::new(),
            screensaver: boompi_proto::ScreensaverKind::default(),
            screensaver_min: default_screensaver_min(),
        }
    }
}

/// Load config from `path`, falling back to a read-only `seed` when the
/// primary file doesn't exist yet.
///
/// The appliance splits config in two: `/etc/boompi/boompi.toml` (image-
/// baked defaults) seeds the very first boot,
/// after which everything persists to `/data/boompi.toml` - which survives
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

/// Load the fully layered appliance config: the runtime config (or
/// its image seed) as the base, with the box hardware profile merged
/// over it.
///
/// The profile (`/data/box/hardware.toml`) describes one physical
/// build - battery wiring, panel DPI seed, amp GPIO - and wins for
/// exactly the keys it specifies. It survives OS updates and factory
/// resets; user-editable runtime settings continue to live in the
/// base config.
pub fn load_layered(
    path: Option<&Path>,
    seed: Option<&Path>,
    hardware: Option<&Path>,
) -> anyhow::Result<Config> {
    let hw = match hardware {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(raw) => Some((p, raw)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("reading hardware profile {}", p.display()))
            }
        },
        None => None,
    };
    let Some((hw_path, hw_raw)) = hw else {
        return load_with_seed(path, seed);
    };

    let base_raw = [path, seed]
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(std::fs::read_to_string)
        .transpose()
        .context("reading base config")?
        .unwrap_or_default();

    let mut merged: toml::Value = toml::from_str(&base_raw).context("parsing base config")?;
    let mut overlay: toml::Value = toml::from_str(&hw_raw)
        .with_context(|| format!("parsing hardware profile {}", hw_path.display()))?;
    // The profile's [settings] table is a first-boot *seed* (e.g.
    // ui_scale for a high-DPI panel), not an override: once the
    // runtime config exists, the user's choices win. Hardware tables
    // ([battery], ...) always win - wiring is not a preference.
    if path.map(|p| p.exists()).unwrap_or(false) {
        if let toml::Value::Table(t) = &mut overlay {
            t.remove("settings");
        }
    }
    merge_toml(&mut merged, overlay);
    let raw = toml::to_string(&merged).context("serializing merged config")?;
    let (cfg, unknown) = parse(&raw).context("parsing merged config")?;
    for key in &unknown {
        tracing::warn!(%key, "ignoring unknown config key (merged config)");
    }
    tracing::info!(profile = %hw_path.display(), "hardware profile merged");
    Ok(cfg)
}

/// Deep-merge `over` into `base`: tables merge recursively, everything
/// else (scalars, arrays) is replaced.
fn merge_toml(base: &mut toml::Value, over: toml::Value) {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => merge_toml(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (b, o) => *b = o,
    }
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
                let (cfg, unknown) =
                    parse(&raw).with_context(|| format!("parsing config {}", p.display()))?;
                // Unknown keys warn instead of failing: configs written
                // by newer builds (or leftovers from withdrawn features)
                // must never keep the appliance from booting - the worst
                // acceptable outcome of a config skew is a lost setting.
                for key in &unknown {
                    tracing::warn!(%key, path = %p.display(), "ignoring unknown config key");
                }
                Ok(cfg)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %p.display(), "config not found; using defaults");
                Ok(Config::default())
            }
            Err(err) => Err(err).with_context(|| format!("reading config {}", p.display())),
        },
    }
}

/// Parse a config, collecting the paths of unknown keys instead of
/// rejecting them (the caller logs them).
fn parse(raw: &str) -> Result<(Config, Vec<String>), toml::de::Error> {
    let de = toml::de::Deserializer::new(raw);
    let mut unknown = Vec::new();
    let cfg = serde_ignored::deserialize(de, |path| unknown.push(path.to_string()))?;
    Ok((cfg, unknown))
}

/// Persist config to `path` atomically (write sibling temp file + rename).
pub fn save(cfg: &Config, path: &Path) -> anyhow::Result<()> {
    let toml = toml::to_string_pretty(cfg).context("serializing config")?;
    let toml = format!("# Boompi device configuration - managed by boompid.\n{toml}");
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &toml).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_profile_merges_over_base() {
        let dir = std::env::temp_dir().join(format!("boompi-hw-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("boompi.toml");
        let hw = dir.join("hardware.toml");
        std::fs::write(
            &base,
            r#"
            name = "Kitchen Boombox"

            [battery]
            i2c_bus = 1
            min_voltage = 18.0
            "#,
        )
        .unwrap();
        std::fs::write(
            &hw,
            r#"
            [battery]
            i2c_bus = 11
            shutdown_voltage = 18.5
            "#,
        )
        .unwrap();
        let cfg = load_layered(Some(&base), None, Some(&hw)).unwrap();
        // Profile wins for the keys it names...
        let b = cfg.battery.as_ref().unwrap();
        assert_eq!(b.i2c_bus, 11);
        assert_eq!(b.shutdown_voltage, 18.5);
        // ...tables merge instead of replacing...
        assert_eq!(b.min_voltage, 18.0);
        // ...and untouched base keys survive.
        assert_eq!(cfg.name, "Kitchen Boombox");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hardware_profile_settings_seed_only() {
        let dir = std::env::temp_dir().join(format!("boompi-hw-seed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let primary = dir.join("boompi.toml");
        let hw = dir.join("hardware.toml");
        std::fs::write(&hw, "[battery]\ni2c_bus = 11\n\n[settings]\nui_scale = 1.5").unwrap();

        // First boot (no runtime config yet): settings seed applies.
        let cfg = load_layered(Some(&primary), None, Some(&hw)).unwrap();
        assert_eq!(cfg.settings.ui_scale, 1.5);

        // The user changed the scale; the profile must not clobber it,
        // while its hardware facts still win.
        std::fs::write(&primary, "[settings]\nui_scale = 2.0").unwrap();
        let cfg = load_layered(Some(&primary), None, Some(&hw)).unwrap();
        assert_eq!(cfg.settings.ui_scale, 2.0);
        assert_eq!(cfg.battery.as_ref().unwrap().i2c_bus, 11);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_hardware_profile_is_fine() {
        let dir = std::env::temp_dir().join(format!("boompi-hw-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("boompi.toml");
        std::fs::write(&base, "name = \"Solo\"").unwrap();
        let cfg = load_layered(Some(&base), None, Some(&dir.join("nope.toml"))).unwrap();
        assert_eq!(cfg.name, "Solo");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hardware_profile_without_base_config() {
        let dir = std::env::temp_dir().join(format!("boompi-hw-nobase-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let hw = dir.join("hardware.toml");
        std::fs::write(&hw, "[battery]\ni2c_bus = 11").unwrap();
        let cfg = load_layered(Some(&dir.join("nope.toml")), None, Some(&hw)).unwrap();
        assert_eq!(cfg.battery.as_ref().unwrap().i2c_bus, 11);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parses_full_config() {
        let cfg: Config = toml::from_str(
            r#"
            name = "Kitchen Boombox"

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
        std::fs::write(&seed, "name = \"Seeded\"\n").unwrap();

        // Primary missing → seed wins.
        let cfg = load_with_seed(Some(&primary), Some(&seed)).unwrap();
        assert_eq!(cfg.name, "Seeded");

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

#[cfg(test)]
mod compat_tests {
    /// Unknown config keys (leftovers from withdrawn features, or
    /// configs written by newer builds) must parse with a warning
    /// instead of keeping the appliance from booting. The airplay_pin
    /// key here is a real leftover: the withdrawn pairing-code build
    /// (v2.0.0-7d5d003) persisted it.
    #[test]
    fn unknown_keys_are_collected_not_fatal() {
        let raw = r#"
name = "Test"
some_future_toplevel_key = true
[settings]
online_art_fallback = false
theme = "dark"
airplay_model = "WiiM Amp"
airplay_pin = "4016"
ui_scale = 1.5
emoji_font = "noto"
update_channel = "edge"
"#;
        let (cfg, unknown) = super::parse(raw).expect("config with unknown keys must parse");
        assert_eq!(cfg.settings.airplay_model, "WiiM Amp");
        assert!(
            unknown.contains(&"settings.airplay_pin".to_string()),
            "{unknown:?}"
        );
        assert!(
            unknown.contains(&"some_future_toplevel_key".to_string()),
            "{unknown:?}"
        );
        // and unknown keys are not round-tripped back into the file
        let out = toml::to_string(&cfg).unwrap();
        assert!(!out.contains("airplay_pin"));
    }
}
