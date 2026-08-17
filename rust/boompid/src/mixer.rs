//! Per-stream volume mixer: two tracks, one reference sink.
//!
//! The system sink is pinned at reference (1.0) forever. Loudness
//! lives on the streams:
//!
//! - **Music track**: every audio source's stream (Bluetooth's
//!   `bluez_input.*` node, the AirPlay and Spotify bridges' pw-cat
//!   streams tagged `application.name = boompi-music`) follows the
//!   shared music volume - one level across sources, so switching
//!   phones never jumps the loudness. Remote volume commands (AVRCP,
//!   DACP, Spirc) set the same value.
//! - **Game track**: RetroArch's stream follows
//!   `settings.game_volume`, independent of music. No ducking - each
//!   track holds its own level.
//!
//! A 1-second reconcile loop (the same pw-dump + wpctl mechanism the
//! old game-ducking loop used) applies desired volumes to any
//! matching stream that drifts, which also catches streams the
//! moment they appear. Volume changes are persisted to the config
//! debounced (5s of stability) so a slider drag doesn't hammer the
//! flash.
//!
//! NB PipeWire's `channelVolumes` are cubic: wpctl value v lands as
//! v^3 in the dump, so comparisons go through cbrt.

#![cfg(target_os = "linux")]

use crate::state::SharedApp;

pub fn spawn(app: SharedApp) {
    tokio::spawn(async move {
        let mut last_persisted: Option<f32> = None;
        let mut last_change: Option<(f32, tokio::time::Instant)> = None;
        // The volume actually applied to streams: slewed toward the
        // requested value. Upward changes ramp at <=0.15/s so no state
        // bug, stale persist, or reconnect can ever BLAST the room
        // (bench: three incidents, one at 12:30am, wife involved);
        // downward changes apply instantly.
        let mut applied_music: Option<f32> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let (music, game) = {
                let s = app.shared.read().await;
                (s.volume, s.settings.game_volume)
            };
            let slewed = match applied_music {
                Some(prev) if music > prev + 0.15 => prev + 0.15,
                _ => music,
            };
            applied_music = Some(slewed);
            reconcile(slewed, game).await;

            // Debounced persistence: remember the music volume across
            // reboots once the slider settles.
            if last_persisted.is_none() {
                last_persisted = Some(music);
            }
            match last_change {
                Some((v, _)) if (v - music).abs() > 0.001 => {
                    last_change = Some((music, tokio::time::Instant::now()));
                }
                None if last_persisted != Some(music) => {
                    last_change = Some((music, tokio::time::Instant::now()));
                }
                Some((v, t))
                    if t.elapsed() > std::time::Duration::from_secs(5)
                        && last_persisted != Some(v) =>
                {
                    app.persist_config().await;
                    last_persisted = Some(v);
                    last_change = None;
                }
                _ => {}
            }
        }
    });
}

async fn reconcile(music: f32, game: f32) {
    let Ok(out) = tokio::process::Command::new("pw-dump").output().await else {
        return;
    };
    let Ok(objs) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return;
    };
    // Bus-routing safety: the fixed unity/makeup volumes on music
    // source streams assume they feed the music BUS (where music-out
    // carries the actual volume). If routing ever fails - rule
    // missing, bus absent, stream created during a stack restart -
    // a source would play straight into the sink at full level. Map
    // the links; any music stream NOT feeding the bus gets the music
    // volume applied directly instead.
    let bus_id = objs.iter().find_map(|o| {
        let p = o.get("info")?.get("props")?;
        (p.get("node.name")?.as_str()? == "music-bus").then(|| o.get("id")?.as_u64())?
    });
    let mut feeds: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for o in &objs {
        if o.get("type").and_then(|t| t.as_str()) == Some("PipeWire:Interface:Link") {
            if let Some(i) = o.get("info") {
                if let (Some(out_n), Some(in_n)) = (
                    i.get("output-node-id").and_then(|v| v.as_u64()),
                    i.get("input-node-id").and_then(|v| v.as_u64()),
                ) {
                    feeds.insert(out_n, in_n);
                }
            }
        }
    }
    let on_bus = |id: u64| -> bool {
        match (bus_id, feeds.get(&id)) {
            (Some(bus), Some(target)) => *target == bus,
            // Unlinked yet (stream still connecting): treat as
            // on-bus; the next tick re-checks.
            (Some(_), None) => true,
            (None, _) => false,
        }
    };
    for obj in &objs {
        let Some(info) = obj.get("info") else { continue };
        let Some(props) = info.get("props") else { continue };
        let media_class = props
            .get("media.class")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let id = obj.get("id").and_then(|v| v.as_u64());

        // The reference sink: pinned at 1.0, always.
        if media_class == "Audio/Sink" {
            if let (Some(id), Some(current)) = (id, current_volume(info)) {
                if (current - 1.0).abs() > 0.01 {
                    tracing::info!(current, "re-pinning sink to reference volume");
                    set_volume(id, 1.0).await;
                }
            }
            continue;
        }

        if !media_class.starts_with("Stream/Output/Audio") {
            continue;
        }
        let node_name = props
            .get("node.name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let app_name = props
            .get("application.name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let is_music_source =
            node_name.starts_with("bluez_input.") || app_name == "boompi-music";
        let routed = id.map(|i| on_bus(i)).unwrap_or(false);
        if is_music_source && !routed {
            // Escaped the bus: apply the music volume directly so it
            // can never play at fixed full level into the sink.
            if let (Some(id), Some(current)) = (id, current_volume(info)) {
                if (current - music).abs() > 0.01 {
                    tracing::warn!(id, node_name, "music stream NOT on bus; applying music volume directly");
                    set_volume(id, music).await;
                }
            }
            continue;
        }
        let desired = if node_name.starts_with("bluez_input.") {
            // Fixed makeup into the bus, bench-calibrated: identical
            // content measured -13.6 dBFS over Bluetooth vs -9.3 over
            // AirPlay AND Spotify (which agree to 0.1 dB) - iOS
            // reserves ~4.3 dB of SBC headroom even at confirmed-max
            // absolute volume. +4.3 dB = x1.18 in wpctl's cubic scale.
            1.18
        } else if app_name == "boompi-music" {
            // Bridges mix into the bus at unity.
            1.0
        } else if node_name == "music-out" {
            // The loopback out of the music bus: THE music volume.
            music
        } else if app_name == "RetroArch" {
            game
        } else {
            continue;
        };
        let (Some(id), Some(current)) = (id, current_volume(info)) else {
            continue;
        };
        if (current - desired).abs() > 0.01 {
            tracing::debug!(id, current, desired, node_name, app_name, "stream volume");
            set_volume(id, desired).await;
        }
    }
}

/// Current volume in wpctl's scale (cbrt of the linear channel gain).
fn current_volume(info: &serde_json::Value) -> Option<f32> {
    let params = info.get("params")?.get("Props")?.as_array()?;
    for p in params {
        if let Some(vols) = p.get("channelVolumes").and_then(|v| v.as_array()) {
            let linear = vols.first()?.as_f64()? as f32;
            return Some(linear.cbrt());
        }
    }
    None
}

async fn set_volume(id: u64, volume: f32) {
    let _ = tokio::process::Command::new("wpctl")
        .args(["set-volume", &id.to_string(), &format!("{:.3}", volume.clamp(0.0, 1.25))])
        .output()
        .await;
}
