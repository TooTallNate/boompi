//! Album-art palette extraction: dominant vibrant colors drive the
//! panel theme (accent/glow/slider) and a background gradient.
//!
//! Deliberately dependency-free: coarse RGB binning + a vibrance score
//! (population x saturation, penalizing near-black/near-white), then
//! the winning hues are renormalized into ranges that read well on the
//! dark and light themes. Grayscale art returns None - the stock theme
//! is better than a mud-colored one.

use image::RgbaImage;

#[derive(Debug, Clone, Copy)]
pub struct ArtPalette {
    pub accent_dark: slint::Color,
    pub accent2_dark: slint::Color,
    pub accent_light: slint::Color,
    pub accent2_light: slint::Color,
    pub bg_top_dark: slint::Color,
    pub bg_bottom_dark: slint::Color,
    pub bg_top_light: slint::Color,
    pub bg_bottom_light: slint::Color,
}

pub fn extract(img: &RgbaImage) -> Option<ArtPalette> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // ~40k samples regardless of art resolution.
    let step = (((w as u64 * h as u64) / 40_000) as f64).sqrt().max(1.0) as u32;

    // 16 levels per channel -> 4096 bins: population + running sums.
    let mut bins: std::collections::HashMap<u16, (u32, u64, u64, u64)> =
        std::collections::HashMap::new();
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let p = img.get_pixel(x, y).0;
            if p[3] >= 128 {
                let key =
                    ((p[0] as u16 >> 4) << 8) | ((p[1] as u16 >> 4) << 4) | (p[2] as u16 >> 4);
                let e = bins.entry(key).or_insert((0, 0, 0, 0));
                e.0 += 1;
                e.1 += p[0] as u64;
                e.2 += p[1] as u64;
                e.3 += p[2] as u64;
            }
            x += step;
        }
        y += step;
    }

    // Vibrance score per bin.
    let mut scored: Vec<(f32, f32, f32, f32)> = Vec::new(); // (score, h, s, v)
    let mut total = 0u32;
    for (count, r, g, b) in bins.values() {
        total += count;
        let n = *count as f32;
        let (hh, ss, vv) = rgb_to_hsv(
            (*r / *count as u64) as u8,
            (*g / *count as u64) as u8,
            (*b / *count as u64) as u8,
        );
        // Saturation is the star; very dark or blown-out pixels rarely
        // make good accents.
        let luma_weight = if !(0.12..=0.97).contains(&vv) {
            0.05
        } else {
            1.0
        };
        let score = n * ss.powf(1.3) * luma_weight;
        scored.push((score, hh, ss, vv));
    }
    if total == 0 {
        return None;
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let best = scored.first().copied()?;
    // Grayscale/mud art: a meaningful accent needs real saturation in a
    // non-trivial share of the image.
    if best.0 < total as f32 * 0.02 || best.2 < 0.25 {
        return None;
    }
    let (_, h1, s1, v1) = best;
    // Second hue: farthest-scoring bin at least 50deg away; otherwise a
    // synthetic analogous companion.
    let (h2, s2, v2) = scored
        .iter()
        .find(|(sc, hh, ss, _)| *sc > 0.0 && *ss > 0.3 && hue_dist(*hh, h1) > 50.0)
        .map(|(_, hh, ss, vv)| (*hh, *ss, *vv))
        .unwrap_or(((h1 + 45.0) % 360.0, s1, v1));

    let color = |h: f32, s: f32, v: f32| {
        let (r, g, b) = hsv_to_rgb(h, s, v);
        slint::Color::from_rgb_u8(r, g, b)
    };
    Some(ArtPalette {
        // Dark theme: bright and saturated enough to glow on near-black.
        accent_dark: color(h1, s1.clamp(0.55, 1.0), v1.clamp(0.65, 0.95)),
        accent2_dark: color(h2, s2.clamp(0.5, 1.0), v2.clamp(0.6, 0.9)),
        // Light theme: darker ink of the same hue.
        accent_light: color(h1, s1.clamp(0.6, 1.0), 0.5),
        accent2_light: color(h2, s2.clamp(0.55, 1.0), 0.48),
        bg_top_dark: color(h1, (s1 * 0.8).min(0.85), 0.34),
        bg_bottom_dark: color(h2, (s2 * 0.7).min(0.75), 0.13),
        bg_top_light: color(h1, 0.22, 0.98),
        bg_bottom_light: color(h2, 0.34, 0.88),
    })
}

fn hue_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_roundtrip() {
        for (r, g, b) in [(255, 0, 0), (0, 200, 100), (30, 60, 200), (250, 250, 250)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((r as i32 - r2 as i32).abs() <= 2, "{r} vs {r2}");
            assert!((g as i32 - g2 as i32).abs() <= 2);
            assert!((b as i32 - b2 as i32).abs() <= 2);
        }
    }

    #[test]
    fn red_album_yields_red_accent() {
        let img = RgbaImage::from_pixel(64, 64, image::Rgba([200, 30, 40, 255]));
        let p = extract(&img).expect("saturated art extracts");
        let c = p.accent_dark;
        assert!(c.red() > c.green() && c.red() > c.blue(), "{c:?}");
    }

    #[test]
    fn grayscale_album_yields_none() {
        let img = RgbaImage::from_pixel(64, 64, image::Rgba([120, 120, 120, 255]));
        assert!(extract(&img).is_none());
    }
}
