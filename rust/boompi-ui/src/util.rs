//! Formatting helpers and battery chart path generation.

use std::collections::VecDeque;

/// Format milliseconds as `m:ss`.
pub fn fmt_mmss(ms: u32) -> String {
    let total_secs = ms / 1000;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}

/// Rolling battery telemetry window, rendered as SVG path commands for the
/// Slint `Path` element (viewbox 100×100, newest sample at the right edge).
pub struct BatteryHistory {
    /// (unix ms, volts, amps)
    points: VecDeque<(u64, f32, f32)>,
}

/// Rolling window width (matches the v1 chart's 3 minutes).
const WINDOW_MS: u64 = 180_000;

/// Chart domains. Voltage covers the 18–24.98 V pack with margin; amps are
/// symmetric so charging (negative) has room below the axis.
const VOLTS_MIN: f32 = 17.5;
const VOLTS_MAX: f32 = 25.5;
const AMPS_MIN: f32 = -3.0;
const AMPS_MAX: f32 = 3.0;

impl BatteryHistory {
    pub fn new() -> Self {
        Self {
            points: VecDeque::new(),
        }
    }

    pub fn push(&mut self, ts: u64, volts: f32, amps: f32) {
        self.points.push_back((ts, volts, amps));
        let cutoff = ts.saturating_sub(WINDOW_MS);
        while matches!(self.points.front(), Some(&(t, _, _)) if t < cutoff) {
            self.points.pop_front();
        }
    }

    /// Build `(voltage_path, current_path)` SVG command strings.
    pub fn paths(&self) -> (String, String) {
        let Some(&(newest, _, _)) = self.points.back() else {
            return (String::new(), String::new());
        };
        if self.points.len() < 2 {
            return (String::new(), String::new());
        }
        let x = |ts: u64| -> f32 {
            100.0 - (newest.saturating_sub(ts) as f32 / WINDOW_MS as f32 * 100.0).min(100.0)
        };
        let y = |value: f32, min: f32, max: f32| -> f32 {
            100.0 - ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0
        };
        let mut volts = String::new();
        let mut amps = String::new();
        for (i, &(ts, v, a)) in self.points.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let px = x(ts);
            volts.push_str(&format!("{cmd} {px:.1} {:.1} ", y(v, VOLTS_MIN, VOLTS_MAX)));
            amps.push_str(&format!("{cmd} {px:.1} {:.1} ", y(a, AMPS_MIN, AMPS_MAX)));
        }
        (volts, amps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmss() {
        assert_eq!(fmt_mmss(0), "0:00");
        assert_eq!(fmt_mmss(999), "0:00");
        assert_eq!(fmt_mmss(61_000), "1:01");
        assert_eq!(fmt_mmss(224_000), "3:44");
        assert_eq!(fmt_mmss(3_600_000), "60:00");
    }

    #[test]
    fn history_paths() {
        let mut h = BatteryHistory::new();
        assert_eq!(h.paths(), (String::new(), String::new()));

        h.push(1_000, 25.5, 0.0); // top of voltage domain, mid amps
        assert_eq!(h.paths(), (String::new(), String::new())); // one point: no line

        h.push(181_500, 17.5, 3.0); // 180.5s later: first point pruned? cutoff = 1500 > 1000 → pruned
        h.push(182_000, 21.5, -3.0);
        let (v, a) = h.paths();
        assert!(v.starts_with("M "));
        assert!(v.contains(" L "));
        // newest sample sits at x=100
        assert!(v.trim_end().ends_with("100.0 50.0"), "volts path: {v}");
        assert!(a.trim_end().ends_with("100.0 100.0"), "amps path: {a}");
    }

    #[test]
    fn history_prunes_window() {
        let mut h = BatteryHistory::new();
        for i in 0..300u64 {
            h.push(i * 2_000, 20.0, 1.0); // 2s cadence, 600s span
        }
        // Only ~90 samples fit in a 180s window at 2s cadence.
        assert!(h.points.len() <= 92, "len = {}", h.points.len());
    }
}

/// Render `url` as QR pixels (1 px per module + quiet zone) for the panel's
/// "More settings" card. Returned buffer is `Send`; wrap it in a
/// `slint::Image` on the UI thread and display with
/// `image-rendering: pixelated`.
pub fn qr_pixels(url: &str) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    let modules = code.width();
    const QUIET: usize = 2; // the white card behind it provides most of it
    let size = modules + QUIET * 2;
    let mut buf =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(size as u32, size as u32);
    let pixels = buf.make_mut_slice();
    let white = slint::Rgba8Pixel::new(255, 255, 255, 255);
    let black = slint::Rgba8Pixel::new(16, 16, 20, 255);
    pixels.fill(white);
    for y in 0..modules {
        for x in 0..modules {
            if code[(x, y)] == qrcode::Color::Dark {
                pixels[(y + QUIET) * size + (x + QUIET)] = black;
            }
        }
    }
    Some(buf)
}
