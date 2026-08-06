//! Audio spectrum analysis for the visualizer.
//!
//! Replaces v1's custom cava fork: mono PCM in, `BARS` smoothed spectrum
//! bars out (u16 full-scale, ready for the binary WebSocket frame).
//! Platform-independent so it unit-tests everywhere; the PipeWire capture
//! that feeds it lives in `visualizer.rs` (Linux only).

use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Number of spectrum bars (matches v1's cava config).
pub const BARS: usize = 10;

/// FFT window size in samples. At 22.05 kHz this is ~93 ms - enough
/// resolution (~10.8 Hz/bin) to separate the low bands.
pub const FFT_SIZE: usize = 2048;

/// Band edges are log-spaced between these frequencies.
const FREQ_LO: f32 = 45.0;
const FREQ_HI: f32 = 10_000.0;

/// dB mapping: -60 dBFS → 0.0, -6 dBFS → 1.0.
const DB_FLOOR: f32 = -60.0;
const DB_RANGE: f32 = 54.0;

pub struct SpectrumAnalyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// Inclusive bin ranges per bar.
    bands: Vec<(usize, usize)>,
    /// Per-bar smoothed levels (0.0-1.0).
    smoothed: [f32; BARS],
    input: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
}

impl SpectrumAnalyzer {
    pub fn new(sample_rate: f32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        // Hann window.
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let x = std::f32::consts::TAU * i as f32 / FFT_SIZE as f32;
                0.5 * (1.0 - x.cos())
            })
            .collect();

        let bin_hz = sample_rate / FFT_SIZE as f32;
        let edge = |i: usize| -> f32 { FREQ_LO * (FREQ_HI / FREQ_LO).powf(i as f32 / BARS as f32) };
        let bands: Vec<(usize, usize)> = (0..BARS)
            .map(|i| {
                let lo = (edge(i) / bin_hz).round() as usize;
                let hi = ((edge(i + 1) / bin_hz).round() as usize).max(lo + 1);
                (lo.max(1), hi.min(FFT_SIZE / 2))
            })
            .collect();

        let spectrum = fft.make_output_vec();
        Self {
            fft,
            window,
            bands,
            smoothed: [0.0; BARS],
            input: vec![0.0; FFT_SIZE],
            spectrum,
        }
    }

    /// Analyze the most recent `FFT_SIZE` samples (i16 mono) and return the
    /// smoothed bars. Call at the frame rate (~30 fps).
    ///
    /// `volume` (0..=1, the system output volume) shifts the whole display
    /// down by the equivalent dB so the bars show what is audibly playing,
    /// not the pre-volume stream content. The sink monitor we capture taps
    /// before the sink volume, so this is the only place volume can enter
    /// the picture - and it keeps the display consistent across sources.
    pub fn process(&mut self, samples: &[i16], volume: f32) -> [u16; BARS] {
        assert!(samples.len() >= FFT_SIZE, "need at least FFT_SIZE samples");
        let tail = &samples[samples.len() - FFT_SIZE..];
        for (dst, (&s, &w)) in self
            .input
            .iter_mut()
            .zip(tail.iter().zip(self.window.iter()))
        {
            *dst = (s as f32 / 32768.0) * w;
        }
        if self
            .fft
            .process(&mut self.input, &mut self.spectrum)
            .is_err()
        {
            return self.output();
        }

        // Hann coherent gain is 0.5; normalize so a full-scale sine reads
        // ~0 dBFS in its band.
        let scale = 2.0 / (FFT_SIZE as f32 * 0.5);

        for (i, &(lo, hi)) in self.bands.iter().enumerate() {
            let mut peak: f32 = 0.0;
            for bin in &self.spectrum[lo..hi] {
                peak = peak.max(bin.norm() * scale);
            }
            let db = 20.0 * (peak + 1e-9).log10()
                + 20.0 * volume.clamp(0.001, 1.0).log10();
            // Slight tilt: music has less energy up high; lift the top bands
            // so the display looks balanced (cava does similar weighting).
            let tilt = 1.0 + 0.35 * (i as f32 / (BARS - 1) as f32);
            let level = (((db - DB_FLOOR) / DB_RANGE) * tilt).clamp(0.0, 1.0);

            // Fast attack, slower decay. The panel adds its own render-rate
            // tween on top, so keep the attack punchy here - double
            // smoothing reads as lag.
            let prev = self.smoothed[i];
            self.smoothed[i] = if level > prev {
                prev + (level - prev) * 0.7
            } else {
                prev * 0.78
            };
            if self.smoothed[i] < 0.004 {
                self.smoothed[i] = 0.0;
            }
        }
        self.output()
    }

    fn output(&self) -> [u16; BARS] {
        let mut out = [0u16; BARS];
        for (o, &s) in out.iter_mut().zip(self.smoothed.iter()) {
            *o = (s * u16::MAX as f32) as u16;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, amplitude: f32, len: usize) -> Vec<i16> {
        (0..len)
            .map(|i| {
                let t = i as f32 / rate;
                (amplitude * (std::f32::consts::TAU * freq * t).sin() * 32767.0) as i16
            })
            .collect()
    }

    #[test]
    fn sine_lights_the_right_band() {
        let rate = 22_050.0;
        let mut analyzer = SpectrumAnalyzer::new(rate);
        let samples = sine(440.0, rate, 0.5, FFT_SIZE * 2);
        // Run a few frames so smoothing settles.
        let mut bars = [0u16; BARS];
        for _ in 0..8 {
            bars = analyzer.process(&samples, 1.0);
        }
        // 440 Hz falls in the band whose range contains it.
        let edge = |i: usize| 45.0 * (10_000.0f32 / 45.0).powf(i as f32 / BARS as f32);
        let expected = (0..BARS)
            .position(|i| edge(i) <= 440.0 && 440.0 < edge(i + 1))
            .unwrap();
        let loudest = bars.iter().enumerate().max_by_key(|(_, &v)| v).unwrap().0;
        assert_eq!(loudest, expected, "bars: {bars:?}");
        assert!(bars[expected] > 20_000, "bars: {bars:?}");
    }

    #[test]
    fn silence_decays_to_zero() {
        let rate = 22_050.0;
        let mut analyzer = SpectrumAnalyzer::new(rate);
        let loud = sine(1000.0, rate, 0.8, FFT_SIZE * 2);
        analyzer.process(&loud, 1.0);
        let quiet = vec![0i16; FFT_SIZE * 2];
        let mut bars = [u16::MAX; BARS];
        for _ in 0..60 {
            bars = analyzer.process(&quiet, 1.0);
        }
        assert_eq!(bars, [0u16; BARS], "should fully decay");
    }
}
