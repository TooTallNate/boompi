//! System (PipeWire) volume control via `wpctl`.
//!
//! v1 shelled out to `amixer`; the PipeWire equivalent is `wpctl` against
//! the default sink. A native pipewire-rs integration can replace this
//! later without changing callers.

#![cfg(target_os = "linux")]

pub async fn set_system_volume(level: f32) -> anyhow::Result<()> {
    let level = level.clamp(0.0, 1.0);
    let status = tokio::process::Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{level:.3}")])
        .status()
        .await?;
    anyhow::ensure!(status.success(), "wpctl set-volume exited with {status}");
    Ok(())
}

/// 44-byte "streaming" WAV header (RIFF/data sizes = 0xFFFFFFFF, s16le).
///
/// pw-cat on PipeWire 1.2.x (the Buildroot 2026.02 image) has no `--raw`
/// mode — that arrived in 1.4 — so raw PCM on stdin makes it print usage
/// and exit 1 (first Pi 4 bench test of AirPlay/Spotify: silent output,
/// pw-cat respawn loop). Every pw-cat version reads WAV from a pipe via
/// libsndfile, so the audio bridges prepend this header to the stream
/// instead. Verified against 1.2.8 on the box and 1.4.2 on the dev Pi.
pub fn wav_stream_header(rate: u32, channels: u16) -> [u8; 44] {
    let byte_rate = rate * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut hdr = [0u8; 44];
    hdr[0..4].copy_from_slice(b"RIFF");
    hdr[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // unknown total size
    hdr[8..12].copy_from_slice(b"WAVE");
    hdr[12..16].copy_from_slice(b"fmt ");
    hdr[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    hdr[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    hdr[22..24].copy_from_slice(&channels.to_le_bytes());
    hdr[24..28].copy_from_slice(&rate.to_le_bytes());
    hdr[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    hdr[32..34].copy_from_slice(&block_align.to_le_bytes());
    hdr[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits per sample
    hdr[36..40].copy_from_slice(b"data");
    hdr[40..44].copy_from_slice(&u32::MAX.to_le_bytes()); // unknown data size
    hdr
}

/// Read the current default-sink volume (e.g. at startup).
pub async fn get_system_volume() -> anyhow::Result<f32> {
    let out = tokio::process::Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .await?;
    // Output shape: "Volume: 0.50" (possibly with a "[MUTED]" suffix).
    let text = String::from_utf8_lossy(&out.stdout);
    let volume = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<f32>().ok())
        .ok_or_else(|| anyhow::anyhow!("unparseable wpctl output: {text:?}"))?;
    Ok(volume.clamp(0.0, 1.0))
}
