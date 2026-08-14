//! Panel mount orientation, read from the kernel's DRM
//! "panel orientation" connector property.
//!
//! The box profile's device tree (e.g. `dtparam=rotate=270` on the
//! HyperPixel overlay) is the single declaration of how the display
//! is physically mounted. The kernel cannot rotate the scanout on
//! VC4 (the HVS only reflects, and DPI panels are RAM-less), so it
//! publishes the mount as a *hint* that every renderer must honor -
//! exactly what a desktop compositor does with a sideways monitor.
//! This crate is the one place that hint is read; boompi-ui derives
//! `SLINT_KMS_ROTATION` from it and boompid's game launcher derives
//! RetroArch's `video_rotation`/`menu_rotation`.
//!
//! Orientation-to-rotation mapping is anchored on real hardware:
//! georges' `rotate=270` becomes "Left Side Up" in the kernel
//! (`of_drm_get_panel_orientation`), and 270 (clockwise degrees,
//! slint convention) is the render rotation proven correct on that
//! panel. The remaining cases follow by symmetry.

#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::os::fd::{AsFd, BorrowedFd};

use drm::control::Device as ControlDevice;

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl drm::Device for Card {}
impl ControlDevice for Card {}

/// Clockwise degrees (slint's convention): 0, 90, 180 or 270.
/// `None` when no connected connector carries the property (HDMI
/// boxes, dev machines) - callers should treat that as 0.
pub fn orientation_degrees() -> Option<u32> {
    let entries = fs::read_dir("/dev/dri").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("card") {
            continue;
        }
        let Ok(file) = File::open(entry.path()) else {
            continue;
        };
        let card = Card(file);
        if let Some(deg) = card_orientation(&card) {
            return Some(deg);
        }
    }
    None
}

fn card_orientation(card: &Card) -> Option<u32> {
    let res = card.resource_handles().ok()?;
    for conn in res.connectors() {
        let Ok(info) = card.get_connector(*conn, false) else {
            continue;
        };
        if info.state() != drm::control::connector::State::Connected {
            continue;
        }
        let Ok(props) = card.get_properties(*conn) else {
            continue;
        };
        for (prop_handle, raw_value) in props.iter() {
            let Ok(prop) = card.get_property(*prop_handle) else {
                continue;
            };
            if prop.name().to_str() != Ok("panel orientation") {
                continue;
            }
            if let drm::control::property::ValueType::Enum(values) =
                prop.value_type()
            {
                let (raw_values, enums) = values.values();
                for (raw, ev) in raw_values.iter().zip(enums.iter()) {
                    if raw != raw_value {
                        continue;
                    }
                    return Some(match ev.name().to_str() {
                        // dtparam rotate=270 -> kernel "Left Side Up"
                        // -> the 270 proven correct on georges.
                        Ok("Left Side Up") => 270,
                        Ok("Right Side Up") => 90,
                        Ok("Upside Down") => 180,
                        _ => 0, // "Normal" / "Unknown"
                    });
                }
            }
        }
    }
    None
}
