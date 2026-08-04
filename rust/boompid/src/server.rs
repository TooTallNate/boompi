//! axum HTTP + WebSocket server.
//!
//! One router, up to two listeners: the protocol port (default :3001,
//! WebSocket + art + JSON API) and — for the browser settings UI — plain
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

/// The Vite/React settings SPA (`web/dist`, committed prebuilt so cargo
/// and Buildroot builds don't need a Node toolchain; `make web` refreshes).
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
        .route("/api/clock", get(api_clock).post(api_clock_set))
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
        None => axum::serve(listener, router)
            .with_graceful_shutdown(shutdown())
            .await?,
    }
    Ok(())
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

async fn hello(app: &SharedApp) -> Hello {
    Hello {
        proto_version: PROTO_VERSION,
        name: app.speaker_name().await,
        model: app.cfg.model.clone(),
        version: crate::state::VERSION.into(),
        uptime_secs: app.started.elapsed().as_secs(),
        settings_url: app.settings_url(),
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

async fn api_clock_set(Json(patch): Json<ClockPatch>) -> axum::response::Response {
    #[cfg(target_os = "linux")]
    match crate::clock::set(patch.timezone.as_deref(), patch.ntp).await {
        Ok(()) => return api_clock().await,
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

/// Accept any [`ClientMessage`] over plain HTTP — same dispatch as the
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
