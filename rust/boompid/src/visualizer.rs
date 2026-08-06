//! Visualizer feed: capture the default sink's monitor via `pw-record`,
//! run the spectrum analyzer, broadcast binary bar frames at ~30 fps.
//!
//! This replaces the v1 architecture entirely (custom cava fork piping
//! over fd 3). Capturing the *sink monitor* means we see whatever is
//! playing - Bluetooth today, librespot/shairport in Phase 3 - after
//! mixing, with no per-source work.

#![cfg(target_os = "linux")]

use crate::dsp::{SpectrumAnalyzer, BARS, FFT_SIZE};
use crate::state::SharedApp;
use boompi_proto::encode_visualizer_frame;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

const SAMPLE_RATE: u32 = 22_050;
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

pub fn spawn(app: SharedApp) {
    tokio::spawn(async move {
        loop {
            match capture(&app).await {
                Ok(()) => tracing::warn!("pw-record stream ended; restarting in 3s"),
                Err(err) => tracing::warn!(%err, "visualizer capture failed; retrying in 3s"),
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

async fn capture(app: &SharedApp) -> anyhow::Result<()> {
    let mut child = tokio::process::Command::new("pw-record")
        .args([
            "--format",
            "s16",
            "--rate",
            &SAMPLE_RATE.to_string(),
            "--channels",
            "1",
            "-P",
            "{ stream.capture.sink = true }",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    tracing::info!("visualizer: capturing default sink monitor at {SAMPLE_RATE} Hz");

    let mut analyzer = SpectrumAnalyzer::new(SAMPLE_RATE as f32);
    // Rolling window of the most recent samples.
    let mut ring = vec![0i16; FFT_SIZE];
    let mut byte_buf = vec![0u8; 4096];
    let mut leftover: Option<u8> = None;
    let mut last_frame = tokio::time::Instant::now();
    let mut was_active = false;

    loop {
        let n = stdout.read(&mut byte_buf).await?;
        if n == 0 {
            return Ok(()); // pw-record exited (e.g. pipewire restart)
        }

        // Bytes → i16 LE samples (handling an odd split across reads).
        let mut bytes = &byte_buf[..n];
        let mut samples = Vec::with_capacity(n / 2 + 1);
        if let Some(hi_pending) = leftover.take() {
            samples.push(i16::from_le_bytes([hi_pending, bytes[0]]));
            bytes = &bytes[1..];
        }
        for pair in bytes.chunks_exact(2) {
            samples.push(i16::from_le_bytes([pair[0], pair[1]]));
        }
        if bytes.len() % 2 == 1 {
            leftover = Some(bytes[bytes.len() - 1]);
        }

        // Slide the ring window.
        if samples.len() >= FFT_SIZE {
            ring.copy_from_slice(&samples[samples.len() - FFT_SIZE..]);
        } else {
            ring.rotate_left(samples.len());
            let start = FFT_SIZE - samples.len();
            ring[start..].copy_from_slice(&samples);
        }

        if last_frame.elapsed() >= FRAME_INTERVAL {
            last_frame = tokio::time::Instant::now();
            // Offset by the *sink* volume: for Bluetooth the phone's
            // volume is already inside the samples and the sink sits at
            // reference, so the capture equals the audible output on
            // every source.
            let volume = app.shared.read().await.sink_volume;
            let bars = analyzer.process(&ring, volume);
            let active = bars.iter().any(|&b| b > 0);
            if active {
                was_active = true;
                app.broadcast_frame(encode_visualizer_frame(&bars));
            } else if was_active {
                // One frame of silence on the transition, then go quiet
                // (v1 suppressed silent cava frames the same way).
                was_active = false;
                app.broadcast_frame(encode_visualizer_frame(&[0u16; BARS]));
            }
        }
    }
}
