//! boompi-ui — the Boompi touchscreen UI.
//!
//! A Slint application that talks to `boompid` over WebSocket. The same
//! binary runs:
//!
//! - on the boombox: Slint `linuxkms` backend (DRM/KMS + libinput, no
//!   compositor), connecting to `ws://127.0.0.1:3001/ws`
//! - on a laptop for development: default winit backend, pointed at a real
//!   boombox (`--backend ws://boombox.local:3001/ws`) or at a local
//!   `boompid --sim`.

mod net;
mod util;

slint::include_modules!();

use boompi_proto::{ClientMessage, PairingAction, SettingsPatch};
use chrono::Local;
use clap::Parser;
use net::{NetCtx, TrackSnap};
use slint::ComponentHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Parser)]
#[command(name = "boompi-ui", version, about)]
struct Cli {
    /// WebSocket URL of the boompid backend.
    #[arg(long, default_value = "ws://127.0.0.1:3001/ws")]
    backend: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ui = AppWindow::new()?;
    let (tx, rx) = mpsc::unbounded_channel::<ClientMessage>();

    let track: Arc<Mutex<Option<TrackSnap>>> = Arc::new(Mutex::new(None));
    let fast_poll = Arc::new(AtomicBool::new(false));

    // ---- UI callbacks → command channel -----------------------------------
    {
        let tx = tx.clone();
        ui.on_previous(move || {
            let _ = tx.send(ClientMessage::Previous);
        });
    }
    {
        let tx = tx.clone();
        ui.on_next(move || {
            let _ = tx.send(ClientMessage::Next);
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.on_toggle_play(move || {
            if let Some(ui) = weak.upgrade() {
                let msg = if ui.get_playing() {
                    ClientMessage::Pause
                } else {
                    ClientMessage::Play
                };
                let _ = tx.send(msg);
            }
        });
    }
    {
        let tx = tx.clone();
        ui.on_volume_edited(move |level| {
            let _ = tx.send(ClientMessage::SetVolume { level });
        });
    }
    {
        // Battery fast-polling follows the battery screen's visibility.
        let tx = tx.clone();
        let weak = ui.as_weak();
        let fast_poll = fast_poll.clone();
        ui.on_screen_changed(move || {
            if let Some(ui) = weak.upgrade() {
                let enabled = ui.get_screen() == "battery";
                if fast_poll.swap(enabled, Ordering::Relaxed) != enabled {
                    let _ = tx.send(ClientMessage::BatteryFastPoll { enabled });
                }
            }
        });
    }
    {
        let tx = tx.clone();
        ui.on_online_art_toggled(move |enabled| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                online_art_fallback: Some(enabled),
                name: None,
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_pairing_enable(move || {
            let _ = tx.send(ClientMessage::Pairing {
                action: PairingAction::Enable,
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_pairing_cancel(move || {
            let _ = tx.send(ClientMessage::Pairing {
                action: PairingAction::Cancel,
            });
        });
    }

    // ---- clock (blinking colon, like v1) -----------------------------------
    let clock_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(250),
            move || {
                if let Some(ui) = weak.upgrade() {
                    let now = Local::now();
                    ui.set_clock_day(now.format("%a").to_string().into());
                    ui.set_clock_h(now.format("%-I").to_string().into());
                    ui.set_clock_m(now.format("%M").to_string().into());
                    ui.set_colon_on(chrono::Timelike::nanosecond(&now) < 500_000_000);
                }
            },
        );
    }

    // ---- track position interpolation (like v1's client-side timers) ------
    let position_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        let track = track.clone();
        position_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(200),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                let snap = *track.lock().unwrap();
                match snap {
                    Some(s) => {
                        let now = chrono::Utc::now().timestamp_millis() as u64;
                        let mut pos = s.position_ms as u64;
                        if s.playing {
                            pos += now.saturating_sub(s.updated_at);
                        }
                        match s.duration_ms {
                            Some(dur) if dur > 0 => {
                                let pos = pos.min(dur as u64) as u32;
                                ui.set_progress(pos as f32 / dur as f32);
                                ui.set_elapsed(util::fmt_mmss(pos).into());
                                ui.set_remaining(format!("-{}", util::fmt_mmss(dur - pos)).into());
                            }
                            _ => {
                                ui.set_progress(0.0);
                                ui.set_elapsed(util::fmt_mmss(pos as u32).into());
                                ui.set_remaining("--:--".into());
                            }
                        }
                    }
                    None => {
                        ui.set_progress(0.0);
                        ui.set_elapsed("--:--".into());
                        ui.set_remaining("--:--".into());
                    }
                }
            },
        );
    }

    // ---- networking thread --------------------------------------------------
    let ctx = NetCtx {
        weak: ui.as_weak(),
        track,
        fast_poll,
    };
    let backend = cli.backend.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(net::network_loop(backend, ctx, rx));
    });

    ui.run()?;
    Ok(())
}
