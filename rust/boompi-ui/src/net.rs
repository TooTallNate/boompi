//! WebSocket client: connects to boompid, applies server messages to the UI,
//! forwards user commands.

use crate::util::BatteryHistory;
use crate::AppWindow;
use boompi_proto::{
    decode_artwork_frame, decode_visualizer_frame, frame_tag, Battery, ClientMessage, PairingState,
    PlaybackStatus, ServerMessage, SourceInfo, SourceKind, Track,
};
use futures_util::{SinkExt, StreamExt};
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Latest track snapshot for client-side position interpolation
/// (read by a UI timer in main.rs).
#[derive(Debug, Clone, Copy)]
pub struct TrackSnap {
    pub position_ms: u32,
    pub duration_ms: Option<u32>,
    pub updated_at: u64,
    pub playing: bool,
}

pub struct NetCtx {
    pub weak: Weak<AppWindow>,
    pub track: Arc<Mutex<Option<TrackSnap>>>,
    pub fast_poll: Arc<AtomicBool>,
}

/// Connect (and reconnect forever) to the backend.
pub async fn network_loop(
    url: String,
    ctx: NetCtx,
    mut rx: mpsc::UnboundedReceiver<ClientMessage>,
) {
    let mut history = BatteryHistory::new();
    loop {
        set_connected(&ctx, false);
        match connect_async(url.as_str()).await {
            Ok((stream, _)) => {
                eprintln!("connected to {url}");
                set_connected(&ctx, true);
                session(stream, &ctx, &mut history, &mut rx).await;
                eprintln!("disconnected from {url}");
            }
            Err(err) => eprintln!("connect {url}: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn session(
    stream: WsStream,
    ctx: &NetCtx,
    history: &mut BatteryHistory,
    rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
) {
    let (mut sink, mut source) = stream.split();

    // Re-assert fast polling if the battery screen is open across a reconnect.
    if ctx.fast_poll.load(Ordering::Relaxed) {
        let msg = ClientMessage::BatteryFastPoll { enabled: true };
        let json = serde_json::to_string(&msg).expect("serialize");
        if sink.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            incoming = source.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(msg) => apply(ctx, history, msg),
                        Err(err) => eprintln!("unparseable server message: {err}"),
                    }
                }
                Some(Ok(Message::Binary(data))) => match data.first() {
                    Some(&frame_tag::VISUALIZER) => {
                        if let Some(bars) = decode_visualizer_frame(&data) {
                            let bars: Vec<f32> = bars
                                .iter()
                                .map(|&b| b as f32 / u16::MAX as f32)
                                .collect();
                            // Raw targets only: the render-rate tween in
                            // main.rs turns these into `bars` + `peaks`.
                            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                                ui.set_bar_targets(ModelRc::new(VecModel::from(bars)));
                            });
                        }
                    }
                    Some(&frame_tag::ARTWORK) => {
                        if let Some(payload) = decode_artwork_frame(&data) {
                            apply_artwork(ctx, payload);
                        }
                    }
                    _ => {}
                },
                Some(Ok(Message::Close(_))) | None => return,
                Some(Ok(_)) => {} // ping/pong
                Some(Err(err)) => {
                    eprintln!("websocket error: {err}");
                    return;
                }
            },
            outgoing = rx.recv() => match outgoing {
                Some(cmd) => {
                    let json = serde_json::to_string(&cmd).expect("serialize command");
                    if sink.send(Message::Text(json)).await.is_err() {
                        return;
                    }
                }
                None => return,
            },
        }
    }
}

fn apply(ctx: &NetCtx, history: &mut BatteryHistory, msg: ServerMessage) {
    match msg {
        ServerMessage::Hello(hello) => {
            let version_line = format!(
                "boompid v{} · protocol v{}",
                hello.version, hello.proto_version
            );
            // QR pixels are generated off-thread (SharedPixelBuffer is
            // Send); the slint::Image itself must be built on the UI thread.
            let qr = hello
                .settings_url
                .as_deref()
                .and_then(crate::util::qr_pixels);
            let settings_url = hello.settings_url.clone().unwrap_or_default();
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                ui.set_speaker_name(hello.name.into());
                ui.set_version_line(version_line.into());
                ui.set_settings_url(settings_url.into());
                if let Some(buf) = qr {
                    ui.set_settings_qr(slint::Image::from_rgba8(buf));
                }
            });
        }
        ServerMessage::State(state) => {
            apply_source(ctx, &state.source);
            match state.track {
                Some(track) => apply_track(ctx, track),
                None => clear_track(ctx),
            }
            if let Some(battery) = state.battery {
                apply_battery(ctx, history, battery);
            }
            apply_games(ctx, state.games.clone());
            let battery_status = match state.battery_status {
                boompi_proto::BatteryStatus::Unconfigured => "unconfigured",
                boompi_proto::BatteryStatus::Error => "error",
                boompi_proto::BatteryStatus::Ok => "ok",
            };
            let battery_detail = state.battery_status_detail.clone().unwrap_or_default();
            {
                let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                    ui.set_battery_status(battery_status.into());
                    ui.set_battery_status_detail(battery_detail.into());
                });
            }
            let pairing = pairing_str(state.pairing.state);
            let online_art = state.settings.online_art_fallback;
            let light = state.settings.theme == boompi_proto::Theme::Light;
            let setup_required = state.setup.required;
            let (wifi_kind, wifi_text) = wifi_status_strings(&state.setup.wifi_status);
            let volume = state.volume;
            let scale = ui_scale(state.settings.ui_scale);
            let emoji = state.emoji_fonts.clone();
            let updates = state.updates.clone();
            let edge = state.settings.update_channel == boompi_proto::UpdateChannel::Edge;
            let airplay_model = state.settings.airplay_model.clone();
            let saver_kind = screensaver_kind_str(state.settings.screensaver);
            let saver_min = state.settings.screensaver_min.min(240) as i32;
            let clock_24h = state.settings.clock_24h;
            apply_wifi(ctx, state.wifi.clone());
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                apply_emoji_fonts(&ui, &emoji);
                apply_update(&ui, &updates);
                ui.set_update_channel_edge(edge);
                ui.set_airplay_model(airplay_model.into());
                ui.set_saver_kind(saver_kind.into());
                ui.set_saver_timeout_min(saver_min);
                ui.set_clock_24h(clock_24h);
                ui.set_volume(volume);
                ui.set_pairing_state(pairing.into());
                ui.set_online_art(online_art);
                ui.set_setup_required(setup_required);
                ui.set_setup_wifi_kind(wifi_kind.into());
                ui.set_setup_wifi_text(wifi_text.into());
                ui.global::<crate::Theme>().set_light(light);
                ui.global::<crate::Theme>().set_scale(scale);
            });
        }
        ServerMessage::Track(track) => apply_track(ctx, track),
        ServerMessage::Source(source) => apply_source(ctx, &source),
        ServerMessage::Volume { level } => {
            let _ = ctx
                .weak
                .upgrade_in_event_loop(move |ui| ui.set_volume(level));
        }
        ServerMessage::Battery(battery) => apply_battery(ctx, history, battery),
        ServerMessage::Games(games) => apply_games(ctx, games),
        ServerMessage::PowerOff { reason, in_secs: _ } => {
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                ui.set_saver_active(false);
                ui.set_poweroff_reason(reason.into());
                ui.set_poweroff_active(true);
            });
        }
        ServerMessage::Pairing(pairing) => {
            let state = pairing_str(pairing.state);
            let device = pairing.device_name.unwrap_or_default();
            let passkey = pairing
                .passkey
                .map(|p| format!("{p:06}"))
                .unwrap_or_default();
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                ui.set_pairing_state(state.into());
                ui.set_pairing_device(device.into());
                ui.set_pairing_passkey(passkey.into());
            });
        }
        ServerMessage::Settings(settings) => {
            let light = settings.theme == boompi_proto::Theme::Light;
            let scale = ui_scale(settings.ui_scale);
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                // Keep the displayed speaker name live: Hello only arrives
                // on (re)connect, so a rename mid-session (e.g. during
                // OOBE) must land through this broadcast too.
                ui.set_speaker_name(settings.name.into());
                ui.set_online_art(settings.online_art_fallback);
                ui.set_airplay_model(settings.airplay_model.into());
                ui.set_saver_kind(screensaver_kind_str(settings.screensaver).into());
                ui.set_saver_timeout_min(settings.screensaver_min.min(240) as i32);
                ui.set_clock_24h(settings.clock_24h);
                ui.set_update_channel_edge(
                    settings.update_channel == boompi_proto::UpdateChannel::Edge,
                );
                ui.global::<crate::Theme>().set_light(light);
                ui.global::<crate::Theme>().set_scale(scale);
            });
        }
        ServerMessage::BtDevices { .. } => {} // panel device list: future work
        ServerMessage::Wifi(wifi) => apply_wifi(ctx, wifi),
        ServerMessage::EmojiFonts(state) => {
            let _ = ctx
                .weak
                .upgrade_in_event_loop(move |ui| apply_emoji_fonts(&ui, &state));
        }
        ServerMessage::ScreensaverPreview => {
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                if ui.get_saver_kind() != "off" {
                    ui.set_saver_active(true);
                }
            });
        }
        ServerMessage::Update(state) => {
            let _ = ctx
                .weak
                .upgrade_in_event_loop(move |ui| apply_update(&ui, &state));
        }
        ServerMessage::Setup(setup) => {
            let (kind, text) = wifi_status_strings(&setup.wifi_status);
            let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
                ui.set_setup_required(setup.required);
                ui.set_setup_wifi_kind(kind.into());
                ui.set_setup_wifi_text(text.into());
            });
        }
    }
}

fn apply_track(ctx: &NetCtx, track: Track) {
    let playing = track.status == PlaybackStatus::Playing;
    *ctx.track.lock().unwrap() = Some(TrackSnap {
        position_ms: track.position_ms.unwrap_or(0),
        duration_ms: track.duration_ms,
        updated_at: track.updated_at,
        playing,
    });
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_has_track(true);
        ui.set_track_title(track.title.unwrap_or_default().into());
        ui.set_track_artist(track.artist.unwrap_or_default().into());
        ui.set_track_album(track.album.unwrap_or_default().into());
        ui.set_playing(playing);
        // Art arrives separately as a binary frame; a track without an
        // artwork_id has none (or none *yet*) - show the placeholder.
        if track.artwork_id.is_none() {
            if ui.get_has_artwork() {
                eprintln!(
                    "artwork cleared by Track update without artwork_id (title={:?})",
                    ui.get_track_title()
                );
            }
            ui.set_has_artwork(false);
            ui.global::<crate::Theme>().set_art_active(false);
        }
    });
}

/// Decode an artwork frame off the UI thread, hand pixels to Slint.
fn apply_artwork(ctx: &NetCtx, payload: &[u8]) {
    if payload.is_empty() {
        let _ = ctx.weak.upgrade_in_event_loop(|ui| {
            ui.set_has_artwork(false);
            ui.global::<crate::Theme>().set_art_active(false);
        });
        return;
    }
    let decoded = match image::load_from_memory(payload) {
        Ok(img) => img.into_rgba8(),
        Err(err) => {
            eprintln!("artwork decode failed: {err}");
            return;
        }
    };
    // Palette extraction happens here, off the UI thread with the
    // pixels already in hand.
    let palette = crate::palette::extract(&decoded);
    let (width, height) = decoded.dimensions();
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        decoded.as_raw(),
        width,
        height,
    );
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_artwork(slint::Image::from_rgba8(buffer));
        ui.set_has_artwork(true);
        let theme = ui.global::<crate::Theme>();
        match palette {
            Some(p) => {
                theme.set_art_accent_dark(p.accent_dark);
                theme.set_art_accent2_dark(p.accent2_dark);
                theme.set_art_accent_light(p.accent_light);
                theme.set_art_accent2_light(p.accent2_light);
                theme.set_art_bg_top_dark(p.bg_top_dark);
                theme.set_art_bg_bottom_dark(p.bg_bottom_dark);
                theme.set_art_bg_top_light(p.bg_top_light);
                theme.set_art_bg_bottom_light(p.bg_bottom_light);
                theme.set_art_active(true);
            }
            None => theme.set_art_active(false),
        }
    });
}

fn clear_track(ctx: &NetCtx) {
    *ctx.track.lock().unwrap() = None;
    let _ = ctx.weak.upgrade_in_event_loop(|ui| {
        ui.set_has_track(false);
        ui.set_track_title("".into());
        ui.set_track_artist("".into());
        ui.set_track_album("".into());
        ui.set_playing(false);
        ui.set_has_artwork(false);
        ui.global::<crate::Theme>().set_art_active(false);
    });
}

/// Project WifiState into the settings screen's Wi-Fi card. Also keeps
/// the settings QR current: toggling the hotspot changes the reachable
/// URL (LAN IP ↔ 10.42.0.1) and Hello only arrives on (re)connect.
fn apply_wifi(ctx: &NetCtx, wifi: boompi_proto::WifiState) {
    // QR pixels off the UI thread, like the Hello handler.
    let qr = wifi
        .settings_url
        .as_deref()
        .and_then(crate::util::qr_pixels);
    let saved: Vec<slint::SharedString> = wifi
        .saved
        .iter()
        .map(|s| slint::SharedString::from(s.as_str()))
        .collect();
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_wifi_supported(wifi.supported);
        ui.set_wifi_connected(wifi.connected.unwrap_or_default().into());
        ui.set_wifi_ip(wifi.ip.unwrap_or_default().into());
        ui.set_wifi_ap_active(wifi.ap_active);
        ui.set_wifi_ap_ssid(wifi.ap_ssid.unwrap_or_default().into());
        ui.set_wifi_saved(ModelRc::new(VecModel::from(saved)));
        if let Some(url) = wifi.settings_url {
            ui.set_settings_url(url.into());
            match qr {
                Some(buf) => ui.set_settings_qr(slint::Image::from_rgba8(buf)),
                // Never leave a QR pointing at the previous URL: a wrong
                // code is worse than none (the URL text still shows).
                None => ui.set_settings_qr(slint::Image::default()),
            }
        }
    });
}

fn screensaver_kind_str(kind: boompi_proto::ScreensaverKind) -> &'static str {
    match kind {
        boompi_proto::ScreensaverKind::Off => "off",
        boompi_proto::ScreensaverKind::Clock => "clock",
        boompi_proto::ScreensaverKind::Matrix => "matrix",
        boompi_proto::ScreensaverKind::Art => "art",
    }
}

fn apply_source(ctx: &NetCtx, source: &SourceInfo) {
    let device = source.device_name.clone().unwrap_or_default();
    let controllable = source.controllable;
    let kind = match source.active {
        Some(SourceKind::Bluetooth) => "bluetooth",
        Some(SourceKind::Spotify) => "spotify",
        Some(SourceKind::Airplay) => "airplay",
        None => "",
    };
    if source.active.is_none() {
        clear_track(ctx);
    }
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_controls_enabled(controllable);
        ui.set_device_name(device.into());
        ui.set_source_kind(kind.into());
    });
}

fn apply_games(ctx: &NetCtx, games: boompi_proto::GamesState) {
    let entries: Vec<crate::GameEntry> = games
        .games
        .iter()
        .map(|g| crate::GameEntry {
            system: g.system.as_str().into(),
            file: g.file.as_str().into(),
            name: g.name.as_str().into(),
            size_label: if g.size >= 1 << 20 {
                format!("{}MB", g.size >> 20).into()
            } else {
                format!("{}KB", g.size >> 10).into()
            },
        })
        .collect();
    let gamepad = games.gamepad;
    let running = games.running.unwrap_or_default();
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_games(std::rc::Rc::new(slint::VecModel::from(entries)).into());
        ui.set_games_gamepad(gamepad);
        ui.set_game_running(running.into());
    });
}

fn apply_battery(ctx: &NetCtx, history: &mut BatteryHistory, battery: Battery) {
    history.push(battery.ts, battery.voltage, battery.current);
    let (volts_path, amps_path) = history.paths();
    let stat_volts = format!("{:.2}", battery.voltage);
    let stat_amps = format!("{:+.2}", battery.current);
    let stat_watts = format!("{:.1}", battery.power);
    let stat_percent = format!("{:.0}%", battery.percentage * 100.0);
    let stat_time = match battery.time_remaining_secs {
        Some(secs) if secs >= 3600 => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
        Some(secs) => format!("{}m", secs / 60),
        None => "-".into(),
    };
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_battery_present(true);
        ui.set_battery_percentage(battery.percentage);
        ui.set_battery_charging(battery.charging);
        ui.set_battery_full(battery.full);
        if battery.low && !ui.get_battery_low() {
            // Warning edge: wake the screensaver so the banner shows.
            ui.set_saver_active(false);
        }
        ui.set_battery_low(battery.low);
        ui.set_battery_voltage_path(volts_path.into());
        ui.set_battery_current_path(amps_path.into());
        ui.set_stat_volts(stat_volts.into());
        ui.set_stat_amps(stat_amps.into());
        ui.set_stat_watts(stat_watts.into());
        ui.set_stat_percent(stat_percent.into());
        ui.set_stat_time(stat_time.into());
    });
}

/// Sanitize a settings scale: old boompid payloads may omit it (0.0
/// through serde default paths) and a zero scale collapses the UI.
fn ui_scale(scale: f32) -> f32 {
    if scale < 0.5 {
        1.0
    } else {
        scale.clamp(0.5, 3.0)
    }
}

/// Wi-Fi join status → (kind, display text) for the setup screen.
fn wifi_status_strings(status: &Option<boompi_proto::WifiJoinStatus>) -> (String, String) {
    use boompi_proto::WifiJoinStatus as W;
    match status {
        None => (String::new(), String::new()),
        Some(W::Joining { ssid }) => ("joining".into(), format!("Joining “{ssid}”…")),
        Some(W::Joined { ssid }) => ("joined".into(), format!("Joined “{ssid}”!")),
        Some(W::Failed { ssid, reason }) => (
            "failed".into(),
            format!("Couldn't join “{ssid}” - {reason}\nRejoin the hotspot to try again."),
        ),
    }
}

/// Project UpdateState into the settings screen's update card.
fn apply_update(ui: &crate::AppWindow, state: &boompi_proto::UpdateState) {
    let (status, detail) = if let Some(v) = &state.applying {
        let stage = match state.stage {
            Some(boompi_proto::UpdateStage::DownloadingSystem) => "downloading system",
            Some(boompi_proto::UpdateStage::VerifyingSystem) => "verifying system",
            Some(boompi_proto::UpdateStage::DownloadingBoot) => "downloading boot files",
            Some(boompi_proto::UpdateStage::VerifyingBoot) => "verifying boot files",
            Some(boompi_proto::UpdateStage::Restarting) => "restarting",
            None => "preparing",
        };
        (
            "applying",
            format!(
                "Installing {v}: {stage}… {:.0}%",
                state.progress.unwrap_or(0.0) * 100.0
            ),
        )
    } else if let Some(err) = &state.error {
        ("error", err.clone())
    } else if state.checking {
        ("checking", String::new())
    } else if let Some(v) = &state.available {
        ("available", format!("{v} is available"))
    } else {
        ("idle", String::new())
    };
    ui.set_update_version(state.version.clone().into());
    ui.set_update_status(status.into());
    ui.set_update_detail(detail.into());
}

/// Project EmojiFontsState into the settings screen's row model.
fn apply_emoji_fonts(ui: &crate::AppWindow, state: &boompi_proto::EmojiFontsState) {
    use slint::{ModelRc, VecModel};
    let rows: Vec<crate::EmojiFontRow> = state
        .fonts
        .iter()
        .map(|f| {
            let status = if state.downloading.as_deref() == Some(f.id.as_str()) {
                "downloading"
            } else if f.active {
                "active"
            } else if f.installed {
                "installed"
            } else if state.downloading.is_some() {
                "busy" // another download running; hide the Get button
            } else {
                "missing"
            };
            let detail = if status == "downloading" {
                format!("Downloading… {:.0}%", state.progress.unwrap_or(0.0) * 100.0)
            } else if !f.installed && f.size > 0 {
                format!("{} MB download", f.size / 1024 / 1024)
            } else {
                f.license.clone()
            };
            crate::EmojiFontRow {
                id: f.id.clone().into(),
                label: f.label.clone().into(),
                status: status.into(),
                detail: detail.into(),
            }
        })
        .collect();
    ui.set_emoji_fonts(ModelRc::new(VecModel::from(rows)));
}

fn pairing_str(state: PairingState) -> &'static str {
    match state {
        PairingState::Idle => "idle",
        PairingState::Discoverable => "discoverable",
        PairingState::Confirm => "confirm",
        PairingState::Pairing => "pairing",
        PairingState::Unavailable => "unavailable",
    }
}

fn set_connected(ctx: &NetCtx, connected: bool) {
    if !connected {
        *ctx.track.lock().unwrap() = None;
    }
    let _ = ctx.weak.upgrade_in_event_loop(move |ui| {
        ui.set_connected(connected);
        if !connected {
            ui.set_has_track(false);
            ui.set_device_name("".into());
            ui.set_source_kind("".into());
            // Reset the *targets* only: `bars`/`peaks` are owned by the
            // render-rate tween in main.rs (replacing them would orphan
            // its models), and it animates everything down to zero.
            ui.set_bar_targets(ModelRc::new(VecModel::from(Vec::<f32>::new())));
        }
    });
}
