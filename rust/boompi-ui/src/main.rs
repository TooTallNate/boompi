//! boompi-ui - the Boompi touchscreen UI.
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
mod palette;
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

    /// Initial screen (dev/screenshot aid): main | battery | settings.
    #[arg(long, default_value = "main")]
    screen: String,

    /// Window size for desktop preview, e.g. "1024x600" (dev aid for
    /// testing the Pi 4 box's panel resolution; ignored on the KMS
    /// backend, which always uses the physical display size).
    #[arg(long)]
    size: Option<String>,
}

/// Render-rate motion for the spectrum bars. boompid streams raw frames
/// at ~30 fps; drawing them directly looks steppy. A 60 fps timer tweens
/// each bar (fast attack, constant-rate fall) and drives the peak-hold
/// caps: a cap rides the bar's maximum, hangs for a moment, then falls
/// with gravity - the classic analyzer effect.
fn start_bar_tween(ui: &AppWindow) -> slint::Timer {
    use slint::{Model, ModelRc, Timer, TimerMode, VecModel};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    const TICK: Duration = Duration::from_millis(16);
    const ATTACK: f32 = 0.45; // fraction of the gap closed per tick
    const FALL_RATE: f32 = 1.9; // bar fall, full scale per second
    const HOLD: Duration = Duration::from_millis(280);
    const GRAVITY: f32 = 5.0; // cap acceleration, full scale per s^2
    const FADE_TIME: f32 = 0.4; // cap fade-out while falling, seconds

    struct CapState {
        vel: f32,
        held_since: Instant,
    }

    let bars_model = Rc::new(VecModel::<f32>::default());
    let peaks_model = Rc::new(VecModel::<f32>::default());
    let fades_model = Rc::new(VecModel::<f32>::default());
    ui.set_bars(ModelRc::from(bars_model.clone()));
    ui.set_peaks(ModelRc::from(peaks_model.clone()));
    ui.set_peak_fades(ModelRc::from(fades_model.clone()));

    let caps: Rc<RefCell<Vec<CapState>>> = Rc::new(RefCell::new(Vec::new()));
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, TICK, move || {
        let Some(ui) = weak.upgrade() else { return };
        let targets = ui.get_bar_targets();
        // (Re)size on first frame or bar-count change. When targets are
        // absent (backend restart, ws reconnect) keep ticking over the
        // existing bars with target 0 so they decay instead of freezing.
        while bars_model.row_count() < targets.row_count() {
            bars_model.push(0.0);
            peaks_model.push(0.0);
            fades_model.push(1.0);
            caps.borrow_mut().push(CapState {
                vel: 0.0,
                held_since: Instant::now(),
            });
        }
        let n = bars_model.row_count();
        let dt = TICK.as_secs_f32();
        let now = Instant::now();
        let mut caps = caps.borrow_mut();
        for i in 0..n {
            let target = targets.row_data(i).unwrap_or(0.0);
            let cur = bars_model.row_data(i).unwrap_or(0.0);
            let next = if target >= cur {
                cur + (target - cur) * ATTACK
            } else {
                (cur - FALL_RATE * dt).max(target)
            };
            if (next - cur).abs() > 0.001 {
                bars_model.set_row_data(i, next);
            }

            let peak = peaks_model.row_data(i).unwrap_or(0.0);
            let fade = fades_model.row_data(i).unwrap_or(1.0);
            let cap = &mut caps[i];
            let (new_peak, new_fade) = if next >= peak {
                // Riding the bar: fully opaque, hold timer rearmed.
                cap.vel = 0.0;
                cap.held_since = now;
                (next, 1.0)
            } else if now.duration_since(cap.held_since) > HOLD {
                // Falling: accelerate and fade towards transparent.
                cap.vel += GRAVITY * dt;
                (
                    (peak - cap.vel * dt).max(next),
                    (fade - dt / FADE_TIME).max(0.0),
                )
            } else {
                (peak, fade)
            };
            // Drop the cap once everything is silent so it doesn't hover
            // over an empty display.
            let new_peak = if target <= 0.001 && next <= 0.005 {
                0.0
            } else {
                new_peak
            };
            if (new_peak - peak).abs() > 0.001 {
                peaks_model.set_row_data(i, new_peak);
            }
            if (new_fade - fade).abs() > 0.005 {
                fades_model.set_row_data(i, new_fade);
            }
        }
    });
    timer
}

/// Register the bundled Noto Color Emoji (CBDT, pinned - the same file
/// the images ship) as the emoji source, making both boxes render
/// identically regardless of fontconfig state.
///
/// Linux-only, and not by choice: macOS Skia rasterizes through
/// CoreText, which cannot draw CBDT bitmap fonts (selection works -
/// fontique picks our font - but every glyph rasterizes empty), and
/// the reverse holds on the boxes, where the Skia/FreeType prebuilt
/// draws CBDT but not COLRv1. No color emoji format renders on both
/// stacks, so the desktop preview keeps the host's Apple Color Emoji.
#[cfg(target_os = "linux")]
fn register_emoji_fallback() {
    use slint::fontique_010::fontique;
    static NOTO: &[u8] = include_bytes!("../ui/fonts/NotoColorEmoji.ttf");
    let blob = fontique::Blob::new(std::sync::Arc::new(NOTO));
    let mut collection = slint::fontique_010::shared_collection();
    let fonts = collection.register_fonts(blob, None);
    // Emoji inherit the surrounding run's script (Unicode script=Common;
    // parley resolves runs to a real script, defaulting to Latin), so
    // the fallback query is keyed on Latn - a Zsye registration is never
    // consulted. Put the bundled font FIRST for the scripts our text
    // realistically uses, keeping the host's own fallback chained after
    // (a custom fontique entry otherwise replaces system fallback
    // entirely, and Geist doesn't cover everything).
    // Parley routes emoji clusters through the generic Emoji family
    // (consulted BEFORE script fallbacks), which the system backend
    // points at the host's emoji font (Apple Color Emoji on macOS).
    // Overriding the generic is what actually makes the bundled font
    // win everywhere.
    collection.set_generic_families(fontique::GenericFamily::Emoji, fonts.iter().map(|f| f.0));
    // Belt and braces for symbol clusters that don't take the emoji
    // path: put the bundled font first in the script fallbacks our text
    // realistically hits, keeping the host's chain behind it.
    for script in ["Latn", "Cyrl", "Grek", "Zsye", "Zsym"] {
        let key = fontique::FallbackKey::new(fontique::Script::from_str_unchecked(script), None);
        let system: Vec<_> = collection.fallback_families(key).collect();
        collection.set_fallbacks(key, fonts.iter().map(|f| f.0).chain(system.iter().copied()));
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ui = AppWindow::new()?;
    // After AppWindow::new: the font collection needs the platform up.
    #[cfg(target_os = "linux")]
    register_emoji_fallback();
    let _bar_tween = start_bar_tween(&ui);
    ui.set_screen(cli.screen.clone().into());
    if let Some(size) = &cli.size {
        if let Some((w, h)) = size.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                ui.window().set_size(slint::PhysicalSize::new(w, h));
            }
        }
    }
    // Screensaver idle clock: reset by every user interaction. The
    // command-channel wrapper covers all daemon-bound taps; navigation
    // and the saver's waking tap reset it explicitly.
    let last_activity = std::rc::Rc::new(std::cell::Cell::new(std::time::Instant::now()));

    #[derive(Clone)]
    struct ActivitySender {
        tx: mpsc::UnboundedSender<ClientMessage>,
        la: std::rc::Rc<std::cell::Cell<std::time::Instant>>,
    }
    impl ActivitySender {
        fn send(&self, msg: ClientMessage) -> Result<(), mpsc::error::SendError<ClientMessage>> {
            self.la.set(std::time::Instant::now());
            self.tx.send(msg)
        }
    }

    let (raw_tx, rx) = mpsc::unbounded_channel::<ClientMessage>();
    let tx = ActivitySender {
        tx: raw_tx,
        la: last_activity.clone(),
    };

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
        // Throttle slider drags to ~10 volume commands/s: unthrottled,
        // every pointer move queues a system-volume call and the level
        // audibly crawls after the finger. Leading edge fires
        // immediately (snappy first response); a trailing single-shot
        // always delivers the final position.
        let tx = tx.clone();
        let pending = std::rc::Rc::new(std::cell::Cell::new(Option::<f32>::None));
        let last_sent = std::rc::Rc::new(std::cell::Cell::new(
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        ));
        let flush_timer = std::rc::Rc::new(slint::Timer::default());
        const GAP: std::time::Duration = std::time::Duration::from_millis(100);
        ui.on_volume_edited(move |level| {
            let elapsed = last_sent.get().elapsed();
            if elapsed >= GAP {
                last_sent.set(std::time::Instant::now());
                pending.set(None);
                let _ = tx.send(ClientMessage::SetVolume { level });
            } else {
                pending.set(Some(level));
                let tx = tx.clone();
                let pending = pending.clone();
                let last_sent = last_sent.clone();
                flush_timer.start(slint::TimerMode::SingleShot, GAP - elapsed, move || {
                    if let Some(level) = pending.take() {
                        last_sent.set(std::time::Instant::now());
                        let _ = tx.send(ClientMessage::SetVolume { level });
                    }
                });
            }
        });
    }
    {
        // Battery fast-polling follows the battery screen's visibility.
        let tx = tx.clone();
        let weak = ui.as_weak();
        let fast_poll = fast_poll.clone();
        let la = last_activity.clone();
        ui.on_screen_changed(move || {
            la.set(std::time::Instant::now());
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
                ..SettingsPatch::default()
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
    {
        let tx = tx.clone();
        ui.on_pairing_confirm(move || {
            let _ = tx.send(ClientMessage::Pairing {
                action: PairingAction::Confirm,
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_pairing_reject(move || {
            let _ = tx.send(ClientMessage::Pairing {
                action: PairingAction::Reject,
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_emoji_font_tapped(move |id, status| {
            let action = match status.as_str() {
                "installed" => boompi_proto::EmojiFontAction::Select,
                "missing" => boompi_proto::EmojiFontAction::Download,
                _ => return,
            };
            let _ = tx.send(ClientMessage::EmojiFont {
                action,
                id: id.to_string(),
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_airplay_model_tapped(move |model| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                airplay_model: Some(model.to_string()),
                ..SettingsPatch::default()
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_saver_kind_tapped(move |kind| {
            let kind = match kind.as_str() {
                "clock" => boompi_proto::ScreensaverKind::Clock,
                "matrix" => boompi_proto::ScreensaverKind::Matrix,
                "art" => boompi_proto::ScreensaverKind::Art,
                _ => boompi_proto::ScreensaverKind::Off,
            };
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                screensaver: Some(kind),
                ..SettingsPatch::default()
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_clock_24h_toggled(move |v| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                clock_24h: Some(v),
                ..SettingsPatch::default()
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_update_check(move || {
            let _ = tx.send(ClientMessage::Update {
                action: boompi_proto::UpdateAction::Check,
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_update_apply(move || {
            let _ = tx.send(ClientMessage::Update {
                action: boompi_proto::UpdateAction::Apply,
            });
        });
    }
    {
        let tx = tx.clone();
        ui.on_update_channel_toggled(move |edge| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                update_channel: Some(if edge {
                    boompi_proto::UpdateChannel::Edge
                } else {
                    boompi_proto::UpdateChannel::Stable
                }),
                ..SettingsPatch::default()
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_scale_changed(move |scale| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                ui_scale: Some(scale),
                ..SettingsPatch::default()
            }));
        });
    }
    {
        let tx = tx.clone();
        ui.on_theme_toggled(move |light| {
            let _ = tx.send(ClientMessage::SetSettings(SettingsPatch {
                theme: Some(if light {
                    boompi_proto::Theme::Light
                } else {
                    boompi_proto::Theme::Dark
                }),
                ..SettingsPatch::default()
            }));
        });
    }

    // ---- idle screensaver ---------------------------------------------------
    // Idle = no interaction for settings.screensaver_min while nothing
    // plays. Interaction resets come from the callbacks below plus the
    // saver's own waking tap; playback starting also wakes the screen.
    {
        let weak = ui.as_weak();
        let la = last_activity.clone();
        ui.on_saver_dismissed(move || {
            la.set(std::time::Instant::now());
            if let Some(ui) = weak.upgrade() {
                ui.set_saver_active(false);
            }
        });
    }
    let saver_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        let la = last_activity.clone();
        saver_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(5),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                if ui.get_saver_active() {
                    // Playback wakes the screen (a session starting is
                    // the moment the display matters again).
                    if ui.get_playing() {
                        la.set(std::time::Instant::now());
                        ui.set_saver_active(false);
                    }
                    return;
                }
                let kind = ui.get_saver_kind();
                if kind == "off" || ui.get_playing() || ui.get_setup_required() {
                    la.set(std::time::Instant::now());
                    return;
                }
                let timeout =
                    std::time::Duration::from_secs((ui.get_saver_timeout_min().max(1) as u64) * 60);
                if la.get().elapsed() >= timeout {
                    ui.set_saver_active(true);
                }
            },
        );
    }
    // Low-fps tick driving all saver motion; deliberately ~8 fps and
    // idle when the saver is hidden (sustained 60 fps GL for hours is
    // exactly the load that wedges the Pi 3's GPU).
    let saver_tick_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        saver_tick_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(125),
            move || {
                let Some(ui) = weak.upgrade() else { return };
                if ui.get_saver_active() {
                    ui.set_saver_tick(ui.get_saver_tick() + 0.125);
                }
            },
        );
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
                    if ui.get_clock_24h() {
                        ui.set_clock_h(now.format("%H").to_string().into());
                        ui.set_clock_ampm("".into());
                    } else {
                        ui.set_clock_h(now.format("%-I").to_string().into());
                        ui.set_clock_ampm(now.format("%p").to_string().into());
                    }
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
