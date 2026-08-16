//! Simulation mode: fake audio source, battery, and visualizer.
//!
//! Lets the UI be developed anywhere (including macOS) with zero hardware:
//! `boompid --sim`. The data intentionally exercises the same code paths and
//! protocol messages the real sources will use.

use crate::state::{now_ms, SharedApp};
use boompi_proto::{
    encode_visualizer_frame, Battery, PlaybackStatus, ServerMessage, SourceInfo, SourceKind, Track,
};
use std::time::Duration;

const BARS: usize = 10;

struct SimTrack {
    title: &'static str,
    artist: &'static str,
    album: &'static str,
    duration_ms: u32,
}

const PLAYLIST: &[SimTrack] = &[
    SimTrack {
        title: "Harder, Better, Faster, Stronger",
        artist: "Daft Punk",
        album: "Discovery",
        duration_ms: 224_000,
    },
    SimTrack {
        title: "Song 2",
        artist: "Blur",
        album: "Blur",
        duration_ms: 122_000,
    },
    SimTrack {
        title: "Maps",
        artist: "Yeah Yeah Yeahs",
        album: "Fever to Tell",
        duration_ms: 220_000,
    },
    SimTrack {
        title: "Midnight City",
        artist: "M83",
        album: "Hurry Up, We're Dreaming",
        duration_ms: 244_000,
    },
];

pub fn spawn(app: SharedApp) {
    tokio::spawn(seed_wifi(app.clone()));
    tokio::spawn(track_loop(app.clone()));
    tokio::spawn(battery_loop(app.clone()));
    tokio::spawn(visualizer_loop(app));
}

/// Fake Wi-Fi facts so the panel/web Wi-Fi cards are exercisable
/// anywhere; the non-Linux `ClientMessage::Wifi` handler mutates these.
async fn seed_wifi(app: SharedApp) {
    // Let the server bind its UI listener first so settings_url resolves.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let wifi = boompi_proto::WifiState {
        supported: true,
        enabled: true,
        connected: Some("Simulated Wi-Fi".into()),
        ip: Some("192.168.1.42/24".into()),
        ap_active: false,
        ap_ssid: None,
        saved: vec![
            "Simulated Wi-Fi".into(),
            "Cabin".into(),
            "Phone Hotspot".into(),
        ],
        settings_url: app.settings_url(),
    };
    app.publish_wifi(wifi).await;
}

/// Simulates a connected Bluetooth phone cycling through a playlist.
/// Advances when a track finishes or when Next/Previous is received.
async fn track_loop(app: SharedApp) {
    {
        let mut s = app.shared.write().await;
        s.source = SourceInfo {
            active: Some(SourceKind::Bluetooth),
            device_name: Some("Simulated Phone".into()),
            controllable: true,
        };
        let source = s.source.clone();
        drop(s);
        app.broadcast(ServerMessage::Source(source));
    }

    let mut index = 0usize;
    loop {
        let t = &PLAYLIST[index % PLAYLIST.len()];
        let track = Track {
            title: Some(t.title.into()),
            artist: Some(t.artist.into()),
            album: Some(t.album.into()),
            duration_ms: Some(t.duration_ms),
            position_ms: Some(0),
            status: PlaybackStatus::Playing,
            artwork_id: None,
            updated_at: now_ms(),
        };
        {
            let mut s = app.shared.write().await;
            s.track = Some(track.clone());
        }
        app.broadcast(ServerMessage::Track(track));
        tracing::info!(title = t.title, artist = t.artist, "sim: now playing");

        // Wait for track end (checking once a second, since pause stops the
        // clock) or a skip request.
        loop {
            tokio::select! {
                _ = app.sim_skip.notified() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let s = app.shared.read().await;
                    let Some(track) = &s.track else { break };
                    if track.status == PlaybackStatus::Playing {
                        let elapsed = now_ms().saturating_sub(track.updated_at) as u32;
                        let position = track.position_ms.unwrap_or(0).saturating_add(elapsed);
                        if position >= track.duration_ms.unwrap_or(u32::MAX) {
                            break;
                        }
                    }
                }
            }
        }
        index += 1;
    }
}

/// Simulates a slowly discharging ~6S pack with some load wobble.
/// Polls fast (1 Hz) while any client requests it, mirroring real behavior.
async fn battery_loop(app: SharedApp) {
    app.shared.write().await.battery_status = boompi_proto::BatteryStatus::Ok;
    let (min_v, max_v) = match &app.cfg.battery {
        Some(b) => (b.min_voltage, b.max_voltage),
        None => (18.0, 24.98),
    };
    // Drive the real estimator so the sim exercises SoC/time-remaining
    // display paths. Pre-seeded calibration = a pack that has already
    // learned itself.
    let mut estimator = crate::soc::SocEstimator::new(
        crate::soc::SocParams {
            min_voltage: min_v,
            default_full_voltage: max_v,
        },
        crate::soc::Calibration {
            full_voltage: Some(max_v),
            capacity_ah: Some(4.2),
            ..Default::default()
        },
    );
    let mut t: f32 = 0.0;
    loop {
        let fast = app.shared.read().await.fast_poll_clients > 0;
        let interval = if fast { 1.0 } else { 2.0 };
        tokio::time::sleep(Duration::from_secs_f32(interval)).await;
        t += interval;

        // Discharge over ~2 simulated hours, plus load-dependent sag.
        let discharge = (t / 7200.0).min(1.0);
        let wobble = (t / 9.0).sin() * 0.15;
        let voltage = max_v - (max_v - min_v) * discharge + wobble * 0.1;
        let current = 1.4 + wobble; // amps; flip sign to simulate charging
        estimator.update(voltage, current, interval);
        let battery = Battery {
            voltage,
            current,
            power: voltage * current,
            percentage: estimator.soc(),
            charging: current <= -0.02,
            full: estimator.full(),
            low: estimator.soc() <= 0.15,
            time_remaining_secs: estimator.time_remaining_secs(),
            ts: now_ms(),
        };
        {
            let mut s = app.shared.write().await;
            s.battery = Some(battery.clone());
        }
        app.broadcast(ServerMessage::Battery(battery));
    }
}

/// ~30 fps of pleasant fake spectrum bars while "playing".
async fn visualizer_loop(app: SharedApp) {
    let mut interval = tokio::time::interval(Duration::from_millis(33));
    let mut t: f32 = 0.0;
    let mut was_playing = false;
    loop {
        interval.tick().await;
        t += 0.033;

        let playing = matches!(
            app.shared.read().await.track.as_ref().map(|tr| tr.status),
            Some(PlaybackStatus::Playing)
        );
        if !playing {
            // Send one frame of silence on the transition, then go quiet
            // (matches v1, which suppressed silent cava frames).
            if was_playing {
                app.broadcast_frame(encode_visualizer_frame(&[0u16; BARS]));
                was_playing = false;
            }
            continue;
        }
        was_playing = true;

        let bars: Vec<u16> = (0..BARS)
            .map(|i| {
                let i = i as f32;
                // A few interfering "frequencies" per bar, shaped so lows are
                // busier than highs, like real music.
                let a = ((t * (1.3 + i * 0.31)).sin() + 1.0) / 2.0;
                let b = ((t * (3.7 + i * 0.13) + i).sin() + 1.0) / 2.0;
                let shape = 1.0 - (i / BARS as f32) * 0.55;
                let level = (0.15 + 0.85 * a * b) * shape;
                (level.clamp(0.0, 1.0) * u16::MAX as f32) as u16
            })
            .collect();
        app.broadcast_frame(encode_visualizer_frame(&bars));
    }
}
