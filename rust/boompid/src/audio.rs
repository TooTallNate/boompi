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
