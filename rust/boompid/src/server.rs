//! axum HTTP + WebSocket server.
//!
//! One router, up to two listeners: the protocol port (default :3001,
//! WebSocket + art + JSON API) and - for the browser settings UI - plain
//! HTTP on :80 when we can bind it (root on the appliance), falling back
//! to :8080 for unprivileged dev runs.

use crate::state::{Outbound, SharedApp};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use boompi_proto::{ClientMessage, Hello, ServerMessage, SettingsPatch, PROTO_VERSION};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;

/// The Vite/React settings SPA (`web/dist`, a build artifact: run
/// `make web` before building boompid; CI's shared `web` job provides it).
#[derive(rust_embed::Embed)]
#[folder = "../../web/dist"]
struct WebAssets;

pub async fn serve(app: SharedApp, addr: SocketAddr) -> anyhow::Result<()> {
    let router = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/healthz", get(|| async { "ok" }))
        .route("/art/{id}", get(artwork))
        .route("/api/state", get(api_state))
        .route("/api/settings", post(api_settings))
        .route("/api/command", post(api_command))
        .route("/api/box", get(api_box).put(api_box_set))
        .route(
            "/api/games/upload",
            post(api_games_upload).layer(axum::extract::DefaultBodyLimit::max(
                1 << 30, // per-system caps enforced inside
            )),
        )
        .route("/api/games/delete", post(api_games_delete))
        .route("/api/box/lock", post(api_box_lock))
        .route("/api/clock", get(api_clock).post(api_clock_set))
        .route("/api/wifi", get(api_wifi).post(api_wifi_action))
        .route(
            "/api/emoji-fonts",
            get(api_emoji_fonts).post(api_emoji_font_action),
        )
        // Captive-portal detection probes (iOS/Android/Windows). In AP
        // mode the portal dnsmasq resolves every name to us; answering
        // these with a redirect pops the OS "sign in to network" sheet
        // straight into the setup page.
        .route("/hotspot-detect.html", get(captive_redirect))
        .route("/generate_204", get(captive_redirect))
        .route("/gen_204", get(captive_redirect))
        .route("/connecttest.txt", get(captive_redirect))
        .route("/ncsi.txt", get(captive_redirect))
        .fallback(get(static_asset))
        .with_state(app.clone());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr} (WebSocket at /ws)");

    // Second listener for the settings web UI on a browser-friendly port.
    let ui_listener = if addr.port() == 80 {
        None
    } else {
        match tokio::net::TcpListener::bind(("0.0.0.0", 80)).await {
            Ok(l) => {
                tracing::info!("settings UI on http://0.0.0.0:80");
                Some(l)
            }
            Err(err) => match tokio::net::TcpListener::bind(("0.0.0.0", 8080)).await {
                Ok(l) => {
                    tracing::info!(%err, "port 80 unavailable; settings UI on http://0.0.0.0:8080");
                    Some(l)
                }
                Err(err8080) => {
                    tracing::warn!(%err, %err8080, "no settings UI port available");
                    None
                }
            },
        }
    };
    // Remember the browser-facing port for Hello.settings_url / the QR code.
    let ui_port = ui_listener
        .as_ref()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(if addr.port() == 80 { 80 } else { 0 });
    app.settings_port
        .store(ui_port, std::sync::atomic::Ordering::Relaxed);

    let shutdown = || async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
    };
    match ui_listener {
        Some(ui) => {
            let (a, b) = tokio::join!(
                axum::serve(listener, router.clone()).with_graceful_shutdown(shutdown()),
                axum::serve(ui, router).with_graceful_shutdown(shutdown()),
            );
            a?;
            b?;
        }
        None => {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown())
                .await?
        }
    }
    Ok(())
}

async fn captive_redirect() -> impl IntoResponse {
    (StatusCode::FOUND, [("location", "/")])
}

/// Embedded SPA assets; unknown paths fall back to index.html so client-side
/// routes survive a refresh. Hashed assets get immutable caching.
async fn static_asset(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let (file, cache) = match WebAssets::get(path) {
        Some(f) => (
            f,
            if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
        ),
        None => match WebAssets::get("index.html") {
            Some(f) => (f, "no-cache"),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    (
        [
            ("content-type", file.metadata.mimetype().to_string()),
            ("cache-control", cache.to_string()),
        ],
        file.data.into_owned(),
    )
        .into_response()
}

/// Combined hello + state snapshot for the settings web UI.
async fn api_state(State(app): State<SharedApp>) -> impl IntoResponse {
    let hello = hello(&app).await;
    let state = app.snapshot().await;
    Json(serde_json::json!({ "hello": hello, "state": state }))
}

/// Greeting payload (WebSocket connect + the BLE events subscription).
pub(crate) async fn hello(app: &SharedApp) -> Hello {
    Hello {
        proto_version: PROTO_VERSION,
        name: app.speaker_name().await,
        model: crate::state::board_model(),
        // The OS image stamp, not the crate version: changesets bump
        // the image version (web/package.json) while the Rust crates
        // stay at a workspace placeholder - Hello.version showing
        // "2.0.0-dev" forever was the tell.
        version: crate::state::os_version().into(),
        uptime_secs: app.started.elapsed().as_secs(),
        settings_url: app.settings_url(),
        capabilities: capabilities(app).await,
    }
}

/// What this box can do, for UIs that outlive its software (hosted
/// remote, phone apps). Hardware-dependent flags read live state;
/// software flags are static facts about this build.
async fn capabilities(app: &SharedApp) -> Vec<String> {
    use boompi_proto::caps;
    let s = app.shared.read().await;
    let mut out: Vec<String> = [
        caps::WIFI_SCAN,
        caps::GAMES,
        caps::EMOJI_FONTS,
        caps::UPDATES,
        caps::SCREENSAVER,
        caps::HOME_ASSISTANT,
        caps::AIRPLAY,
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    if s.wifi.supported {
        out.push(caps::WIFI.into());
    }
    if s.battery_status != boompi_proto::BatteryStatus::Unconfigured {
        out.push(caps::BATTERY.into());
    }
    if s.pairing.state != boompi_proto::PairingState::Unavailable {
        out.push(caps::BLUETOOTH.into());
    }
    out
}

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum WifiAction {
    Connect {
        ssid: String,
        psk: Option<String>,
    },
    Forget {
        name: String,
    },
    /// Drop the current connection without forgetting it (suppresses
    /// autoconnect until a manual rejoin).
    Disconnect,
    Radio {
        enabled: bool,
    },
    /// The speaker's own hotspot (onboarding + camping mode); the SSID
    /// is the speaker name.
    Ap {
        enabled: bool,
    },
}

async fn api_wifi() -> axum::response::Response {
    #[cfg(target_os = "linux")]
    match crate::wifi::status(true).await {
        Ok(status) => return Json(status).into_response(),
        Err(err) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    }
    #[cfg(not(target_os = "linux"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "wifi control is Linux-only" })),
    )
        .into_response()
}

async fn api_emoji_fonts(State(app): State<SharedApp>) -> axum::response::Response {
    let _ = &app;
    #[cfg(target_os = "linux")]
    {
        return Json(crate::fonts::state(&app).await).into_response();
    }
    #[cfg(not(target_os = "linux"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "emoji fonts are Linux-only" })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct EmojiFontAction {
    action: String,
    id: String,
}

async fn api_emoji_font_action(
    State(app): State<SharedApp>,
    Json(req): Json<EmojiFontAction>,
) -> axum::response::Response {
    let _ = (&app, &req);
    #[cfg(target_os = "linux")]
    {
        let action = match req.action.as_str() {
            "download" => boompi_proto::EmojiFontAction::Download,
            "select" => boompi_proto::EmojiFontAction::Select,
            "remove" => boompi_proto::EmojiFontAction::Remove,
            other => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": format!("unknown action: {other}") })),
                )
                    .into_response();
            }
        };
        return match crate::fonts::perform(&app, action, &req.id).await {
            Ok(()) => api_emoji_fonts(State(app)).await,
            Err(err) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response(),
        };
    }
    #[cfg(not(target_os = "linux"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "emoji fonts are Linux-only" })),
    )
        .into_response()
}

async fn api_wifi_action(
    State(app): State<SharedApp>,
    Json(action): Json<WifiAction>,
) -> axum::response::Response {
    let _ = &app;
    #[cfg(target_os = "linux")]
    {
        let result = match &action {
            WifiAction::Connect { ssid, psk } => {
                app.set_wifi_status(Some(boompi_proto::WifiJoinStatus::Joining {
                    ssid: ssid.clone(),
                }))
                .await;
                let res = crate::wifi::connect(ssid, psk.as_deref()).await;
                match &res {
                    Ok(()) => {
                        app.set_wifi_status(Some(boompi_proto::WifiJoinStatus::Joined {
                            ssid: ssid.clone(),
                        }))
                        .await;
                    }
                    Err(err) => {
                        // nmcli's wrong-password signature is a secrets
                        // failure; everything else gets the raw reason.
                        let msg = err.to_string();
                        let reason = if msg.to_lowercase().contains("secrets") {
                            "wrong password?".to_string()
                        } else {
                            msg.lines().next().unwrap_or("failed").to_string()
                        };
                        app.set_wifi_status(Some(boompi_proto::WifiJoinStatus::Failed {
                            ssid: ssid.clone(),
                            reason,
                        }))
                        .await;
                    }
                }
                // Joining from the captive portal tears the hotspot down
                // (single radio). If the join failed while still in
                // first-boot setup, bring the hotspot back so the phone
                // auto-rejoins and the user can retry the password.
                if res.is_err() && app.snapshot().await.setup.required {
                    let name = app.speaker_name().await;
                    if let Err(err) = crate::wifi::start_ap(&name).await {
                        tracing::warn!(%err, "failed to restore onboarding AP after join failure");
                    }
                }
                // A successful join is the last real onboarding step, and
                // it kills the hotspot the wizard is served over - the
                // "Finish setup" tap can never arrive (the name step is
                // already behind the user; the skip-wifi path still ends
                // via the finish button, where the AP survives).
                if res.is_ok() && app.snapshot().await.setup.required {
                    tracing::info!("wifi joined during setup; completing onboarding");
                    if app.complete_setup().await {
                        app.persist_config().await;
                    }
                }
                res
            }
            WifiAction::Forget { name } => crate::wifi::forget(name).await,
            WifiAction::Disconnect => crate::wifi::disconnect().await,
            WifiAction::Radio { enabled } => crate::wifi::set_radio(*enabled).await,
            WifiAction::Ap { enabled: true } => {
                crate::wifi::start_ap(&app.speaker_name().await).await
            }
            WifiAction::Ap { enabled: false } => crate::wifi::stop_ap().await,
        };
        // Push the change to WebSocket clients too (panel Wi-Fi card).
        app.refresh_wifi().await;
        return match result {
            Ok(()) => api_wifi().await,
            Err(err) => {
                tracing::warn!(%err, "wifi action failed");
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": err.to_string() })),
                )
                    .into_response()
            }
        };
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = action;
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({ "error": "wifi control is Linux-only" })),
        )
            .into_response()
    }
}

#[derive(serde::Deserialize)]
struct ClockPatch {
    timezone: Option<String>,
    ntp: Option<bool>,
}

async fn api_clock() -> axum::response::Response {
    #[cfg(target_os = "linux")]
    match crate::clock::status().await {
        Ok(status) => return Json(status).into_response(),
        Err(err) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    }
    #[cfg(not(target_os = "linux"))]
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({ "error": "clock control is Linux-only" })),
    )
        .into_response()
}

async fn api_clock_set(
    State(app): State<SharedApp>,
    Json(patch): Json<ClockPatch>,
) -> axum::response::Response {
    #[cfg(not(target_os = "linux"))]
    let _ = &app;
    #[cfg(target_os = "linux")]
    match crate::clock::set(patch.timezone.as_deref(), patch.ntp).await {
        Ok(()) => {
            // Persist to /data: /etc/localtime sits on the A/B rootfs and
            // an OTA replaces it; boompid re-applies this copy at startup.
            {
                let mut s = app.shared.write().await;
                if patch.timezone.is_some() {
                    s.timezone = patch.timezone.clone();
                }
                if patch.ntp.is_some() {
                    s.ntp = patch.ntp;
                }
            }
            app.persist_config().await;
            return api_clock().await;
        }
        Err(err) => {
            tracing::warn!(%err, "clock change failed");
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": err.to_string() })),
            )
                .into_response();
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = patch;
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({ "error": "clock control is Linux-only" })),
        )
            .into_response()
    }
}

/// Accept any [`ClientMessage`] over plain HTTP - same dispatch as the
/// WebSocket path. Lets the web UI (and curl) drive pairing/device actions
/// without holding a socket open.
async fn api_command(
    State(app): State<SharedApp>,
    Json(msg): Json<ClientMessage>,
) -> impl IntoResponse {
    app.handle_client_message(msg).await;
    StatusCode::NO_CONTENT
}

/// Apply a settings patch (same semantics as the WebSocket message) and
/// return the resulting settings.
async fn api_settings(
    State(app): State<SharedApp>,
    Json(patch): Json<SettingsPatch>,
) -> impl IntoResponse {
    app.handle_client_message(ClientMessage::SetSettings(patch))
        .await;
    Json(app.snapshot().await.settings)
}

#[derive(serde::Deserialize)]
struct GamesUploadQuery {
    system: String,
}

/// Upload a ROM (multipart, single file field). Streamed to a temp
/// file in the target directory, renamed into place on success -
/// /data has the space (grown to fill the card), tmpfs does not.
async fn api_games_upload(
    State(app): State<SharedApp>,
    axum::extract::Query(q): axum::extract::Query<GamesUploadQuery>,
    mut multipart: axum::extract::Multipart,
) -> axum::response::Response {
    let err = |code: StatusCode, msg: String| {
        (code, Json(serde_json::json!({ "error": msg }))).into_response()
    };
    let system = q.system;
    let is_bios = system == "bios";
    if !is_bios && !crate::games::SYSTEMS.iter().any(|(id, _, _)| *id == system) {
        return err(StatusCode::BAD_REQUEST, format!("unknown system {system}"));
    }
    let cap = crate::games::upload_cap(&system);
    let dir = if is_bios {
        crate::games::games_dir().join("bios")
    } else {
        crate::games::roms_dir().join(&system)
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let Some(raw_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let Some(name) = crate::games::sanitize_file_name(&raw_name) else {
            return err(
                StatusCode::BAD_REQUEST,
                format!("bad file name {raw_name:?}"),
            );
        };
        if !crate::games::upload_extension_ok(&system, &name) {
            return err(
                StatusCode::BAD_REQUEST,
                format!("{name}: extension not accepted for {system}"),
            );
        }
        let tmp = dir.join(format!(".upload-{name}"));
        let final_path = dir.join(&name);
        let mut file = match tokio::fs::File::create(&tmp).await {
            Ok(f) => f,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        let mut written: u64 = 0;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    written += chunk.len() as u64;
                    if written > cap {
                        drop(file);
                        let _ = tokio::fs::remove_file(&tmp).await;
                        return err(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            format!("{name} exceeds the {}MB cap for {system}", cap >> 20),
                        );
                    }
                    if let Err(e) = file.write_all(&chunk).await {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    return err(StatusCode::BAD_REQUEST, e.to_string());
                }
            }
        }
        if let Err(e) = file.sync_all().await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
        drop(file);
        if let Err(e) = tokio::fs::rename(&tmp, &final_path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
        }
        tracing::info!(system, name, written, "game asset uploaded");
    }
    crate::games::refresh(&app).await;
    Json(app.snapshot().await.games).into_response()
}

#[derive(serde::Deserialize)]
struct GamesDeleteBody {
    system: String,
    file: String,
}

async fn api_games_delete(
    State(app): State<SharedApp>,
    Json(body): Json<GamesDeleteBody>,
) -> axum::response::Response {
    let ok = crate::games::sanitize_file_name(&body.file).is_some()
        && crate::games::SYSTEMS
            .iter()
            .any(|(id, _, _)| *id == body.system);
    if !ok {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "bad system or file" })),
        )
            .into_response();
    }
    let path = crate::games::roms_dir().join(&body.system).join(&body.file);
    // Companion cleanup: deleting a .cue takes its .bin tracks along.
    let _ = tokio::fs::remove_file(&path).await;
    if body.file.to_ascii_lowercase().ends_with(".cue") {
        let stem = &body.file[..body.file.len() - 4];
        for ext in ["bin", "img", "ccd", "sub"] {
            let _ = tokio::fs::remove_file(
                crate::games::roms_dir()
                    .join(&body.system)
                    .join(format!("{stem}.{ext}")),
            )
            .await;
        }
    }
    crate::games::refresh(&app).await;
    Json(app.snapshot().await.games).into_response()
}

/// The box profile (/data/box/), as edited by the configurator page.
/// Once locked, hardware config is ssh-only (`boompi-box`): the whole
/// API answers 403 so an unattended LAN curl cannot touch the boot
/// configuration.
async fn api_box() -> axum::response::Response {
    if crate::boxprofile::locked() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "locked": true })),
        )
            .into_response();
    }
    Json(crate::boxprofile::read()).into_response()
}

async fn api_box_set(Json(profile): Json<crate::boxprofile::Profile>) -> axum::response::Response {
    if crate::boxprofile::locked() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "locked": true })),
        )
            .into_response();
    }
    match crate::boxprofile::write(&profile).await {
        Ok(outcome) => (StatusCode::OK, Json(serde_json::json!(outcome))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

/// One-way from the web: unlocking requires ssh (`boompi-box unlock`).
async fn api_box_lock() -> axum::response::Response {
    match crate::boxprofile::lock() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "locked": true }))).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn ws_upgrade(State(app): State<SharedApp>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = client_session(app, socket).await {
            tracing::debug!(%err, "websocket session ended with error");
        }
    })
}

/// Serve cached artwork by id (content-addressed → safely immutable).
async fn artwork(State(app): State<SharedApp>, Path(id): Path<String>) -> impl IntoResponse {
    match app.get_art(&id).await {
        Some(bytes) => (
            StatusCode::OK,
            [
                ("content-type", "image/jpeg"),
                ("cache-control", "public, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn client_session(app: SharedApp, mut socket: WebSocket) -> anyhow::Result<()> {
    let mut rx = app.tx.subscribe();

    // Greeting: hello + full state snapshot.
    let hello = ServerMessage::Hello(hello(&app).await);
    send_json(&mut socket, &hello).await?;
    let snapshot = app.snapshot().await;
    let artwork_id = snapshot.track.as_ref().and_then(|t| t.artwork_id.clone());
    send_json(&mut socket, &ServerMessage::State(snapshot)).await?;
    // Current track's artwork, if any, so late joiners render it too.
    if let Some(id) = artwork_id {
        if let Some(bytes) = app.get_art(&id).await {
            socket
                .send(Message::Binary(
                    boompi_proto::encode_artwork_frame(&bytes).into(),
                ))
                .await?;
        }
    }

    tracing::info!("client connected");
    // Whether *this* connection requested battery fast-polling, so we can
    // release it on disconnect.
    let mut fast_poll = false;

    let result: anyhow::Result<()> = loop {
        tokio::select! {
            out = rx.recv() => match out {
                Ok(Outbound::Message(json)) => {
                    socket.send(Message::Text(json.as_ref().into())).await?
                }
                Ok(Outbound::Frame(frame)) => socket.send(Message::Binary(frame)).await?,
                // Slow client skipped some broadcasts; keep going (a fresh
                // snapshot is only needed on reconnect).
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "client lagged behind broadcasts");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break Ok(()),
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ClientMessage>(text.as_str()) {
                        Ok(ClientMessage::BatteryFastPoll { enabled }) => {
                            if enabled != fast_poll {
                                fast_poll = enabled;
                                app.set_fast_poll(if enabled { 1 } else { -1 }).await;
                            }
                        }
                        Ok(msg) => app.handle_client_message(msg).await,
                        Err(err) => tracing::warn!(%err, %text, "unparseable client message"),
                    }
                }
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Ok(_)) => {} // ping/pong handled by axum; ignore binary
                Some(Err(err)) => break Err(err.into()),
            },
        }
    };

    if fast_poll {
        app.set_fast_poll(-1).await;
    }
    tracing::info!("client disconnected");
    result
}

async fn send_json(socket: &mut WebSocket, msg: &ServerMessage) -> anyhow::Result<()> {
    let json = serde_json::to_string(msg)?;
    socket.send(Message::Text(json.into())).await?;
    Ok(())
}
