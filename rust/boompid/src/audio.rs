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

/// Pin the Bluetooth capture stream's volume back to full scale.
///
/// PipeWire applies AVRCP absolute volume as a soft volume on the
/// `bluez_input.*` node. boompid already maps that same volume onto the
/// system sink, so leaving the node scaled attenuates Bluetooth audio
/// twice (a quadratic loudness taper), and makes the visualizer capture
/// (the pre-volume sink monitor) inconsistent with the other sources.
/// Called after every phone-volume event; WirePlumber may re-apply its
/// scaling in the same event window, so callers schedule a delayed
/// second pass.
pub async fn reset_bt_stream_volume() -> anyhow::Result<()> {
    let out = tokio::process::Command::new("pw-cli")
        .args(["ls", "Node"])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut current_id: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("id ") {
            current_id = rest.split(',').next().map(|s| s.trim().to_string());
        } else if line.contains("node.name") && line.contains("bluez_input") {
            if let Some(id) = current_id.take() {
                let status = tokio::process::Command::new("wpctl")
                    .args(["set-volume", &id, "1.0"])
                    .status()
                    .await?;
                anyhow::ensure!(status.success(), "wpctl set-volume {id} failed");
                tracing::debug!(%id, "bluez stream volume pinned to 1.0");
            }
        }
    }
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
