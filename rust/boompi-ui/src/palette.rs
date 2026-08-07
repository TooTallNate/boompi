//! Album-art palette extraction: dominant vibrant colors drive the
//! panel theme (accent/glow/slider) and a background gradient.
//!
//! Deliberately dependency-free: a saturation-weighted hue histogram
//! (36 buckets, scored over 3-bucket windows with circular means).
//! Judging dominance by hue rather than RGB bins matters for
//! photographic and gradient-heavy covers: their color mass shatters
//! across hundreds of nearby RGB bins (no single bin looks dominant)
//! while concentrating tightly in hue. Near-black/near-white pixels
//! are discounted, and grayscale art returns None - the stock theme
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

#[derive(Clone, Copy, Default)]
struct Bucket {
    mass: f32, // saturation- and luma-weighted sample mass
    sin: f32,  // circular hue mean accumulators (mass-weighted)
    cos: f32,
    sat: f32,
    val: f32,
}

const BUCKETS: usize = 36; // 10 degrees each

/// Mass, circular-mean hue, and weighted sat/val over buckets
/// [i-1, i, i+1] (wrapping).
fn window(buckets: &[Bucket; BUCKETS], i: usize) -> (f32, f32, f32, f32) {
    let (mut m, mut sin, mut cos, mut s, mut v) = (0f32, 0f32, 0f32, 0f32, 0f32);
    for o in [BUCKETS - 1, 0, 1] {
        let b = &buckets[(i + o) % BUCKETS];
        m += b.mass;
        sin += b.sin;
        cos += b.cos;
        s += b.sat;
        v += b.val;
    }
    if m <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let hue = sin.atan2(cos).to_degrees().rem_euclid(360.0);
    (m, hue, s / m, v / m)
}

pub fn extract(img: &RgbaImage) -> Option<ArtPalette> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // ~40k samples regardless of art resolution.
    let step = (((w as u64 * h as u64) / 40_000) as f64).sqrt().max(1.0) as u32;

    let mut buckets = [Bucket::default(); BUCKETS];
    let mut total = 0u32;
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            let p = img.get_pixel(x, y).0;
            if p[3] >= 128 {
                total += 1;
                let (hh, ss, vv) = rgb_to_hsv(p[0], p[1], p[2]);
                // Saturation is the star; very dark or blown-out pixels
                // rarely make good accents.
                let luma_weight = if !(0.12..=0.97).contains(&vv) {
                    0.05
                } else {
                    1.0
                };
                let weight = ss.powf(1.3) * luma_weight;
                if weight > 0.0 {
                    let b = &mut buckets[(hh / 10.0) as usize % BUCKETS];
                    let rad = hh.to_radians();
                    b.mass += weight;
                    b.sin += weight * rad.sin();
                    b.cos += weight * rad.cos();
                    b.sat += weight * ss;
                    b.val += weight * vv;
                }
            }
            x += step;
        }
        y += step;
    }
    if total == 0 {
        return None;
    }

    // Dominant hue arc.
    let i1 =
        (0..BUCKETS).max_by(|&a, &b| window(&buckets, a).0.total_cmp(&window(&buckets, b).0))?;
    let (m1, h1, s1, v1) = window(&buckets, i1);
    // Grayscale/mud art: a meaningful accent needs real saturated mass
    // (3% of the image at full saturation, or equivalent) in one arc.
    if m1 < total as f32 * 0.03 || s1 < 0.25 {
        return None;
    }

    // Second hue: strongest arc at least 50deg away; otherwise a
    // synthetic analogous companion.
    let second = (0..BUCKETS)
        .filter(|&i| {
            let center = i as f32 * 10.0 + 5.0;
            hue_dist(center, h1) > 50.0
        })
        .max_by(|&a, &b| window(&buckets, a).0.total_cmp(&window(&buckets, b).0))
        .map(|i| window(&buckets, i))
        .filter(|&(m, _, s, _)| m > total as f32 * 0.008 && s > 0.25);
    let (h2, s2, v2) = match second {
        Some((_, hh, ss, vv)) => (hh, ss, vv),
        None => ((h1 + 45.0) % 360.0, s1, v1),
    };

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
    fn gradient_album_yields_warm_accent() {
        // Photographic covers spread one hue family across hundreds of
        // RGB bins (the bug this test pins down): a smooth dark-brown ->
        // gold -> pale-yellow wash, no two rows alike.
        let mut img = RgbaImage::new(64, 64);
        for y in 0..64u32 {
            for x in 0..64u32 {
                let t = y as f32 / 63.0;
                let (r, g, b) = super::hsv_to_rgb(
                    35.0 + 15.0 * t + (x % 7) as f32, // amber-ish, jittered
                    0.75 - 0.2 * t,
                    0.15 + 0.8 * t,
                );
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
        let p = extract(&img).expect("gradient art must extract");
        let c = p.accent_dark;
        // Warm accent: red-dominant with green above blue.
        assert!(c.red() >= c.green() && c.green() > c.blue(), "{c:?}");
    }

    #[test]
    fn dark_cover_with_purple_subject_yields_purple() {
        // The "mostly black stage photo" archetype: ~75% noisy
        // near-black, a purple scene (~18%), and bright orange bulbs
        // (~5%). The blacks must not drown the subject, purple must
        // out-mass the (more saturated but smaller) orange, and the
        // orange should surface as the second accent.
        let mut img = RgbaImage::new(100, 100);
        let mut k = 0u32;
        for y in 0..100u32 {
            for x in 0..100u32 {
                k = k.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (k >> 24) as f32 / 255.0;
                let (h, s, v) = if y < 75 {
                    (noise * 360.0, 0.15 * noise, 0.05 + 0.08 * noise) // noisy black
                } else if x < 78 {
                    (
                        275.0 + 25.0 * noise,
                        0.45 + 0.25 * noise,
                        0.2 + 0.35 * noise,
                    ) // purple
                } else if x < 96 {
                    (25.0 + 10.0 * noise, 0.9, 0.85 + 0.1 * noise) // orange bulbs
                } else {
                    (350.0, 0.5, 0.9) // pink title text
                };
                let (r, g, b) = super::hsv_to_rgb(h, s, v);
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
        let p = extract(&img).expect("dark cover with a colorful subject must extract");
        let (h1, _, _) = super::rgb_to_hsv(
            p.accent_dark.red(),
            p.accent_dark.green(),
            p.accent_dark.blue(),
        );
        assert!((250.0..=320.0).contains(&h1), "accent hue {h1} not purple");
        let (h2, _, _) = super::rgb_to_hsv(
            p.accent2_dark.red(),
            p.accent2_dark.green(),
            p.accent2_dark.blue(),
        );
        assert!(
            super::hue_dist(h2, 30.0) < 40.0,
            "accent2 hue {h2} not orange"
        );
    }

    #[test]
    fn grayscale_album_yields_none() {
        let img = RgbaImage::from_pixel(64, 64, image::Rgba([120, 120, 120, 255]));
        assert!(extract(&img).is_none());
    }
}
