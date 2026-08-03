//! axum HTTP + WebSocket server.

use crate::state::{Outbound, SharedApp};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use boompi_proto::{ClientMessage, Hello, ServerMessage, PROTO_VERSION};
use std::net::SocketAddr;

pub async fn serve(app: SharedApp, addr: SocketAddr) -> anyhow::Result<()> {
    let router = Router::new()
        .route("/ws", get(ws_upgrade))
        .route("/healthz", get(|| async { "ok" }))
        .route("/art/{id}", get(artwork))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr} (WebSocket at /ws)");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;
    Ok(())
}

async fn ws_upgrade(State(app): State<SharedApp>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = client_session(app, socket).await {
            tracing::debug!(%err, "websocket session ended with error");
        }
    })
}

/// Serve cached artwork by id. TODO(Phase 3): artwork pipeline + cache.
async fn artwork(Path(id): Path<String>) -> impl IntoResponse {
    tracing::debug!(%id, "artwork requested (pipeline not implemented yet)");
    StatusCode::NOT_FOUND
}

async fn client_session(app: SharedApp, mut socket: WebSocket) -> anyhow::Result<()> {
    let mut rx = app.tx.subscribe();

    // Greeting: hello + full state snapshot.
    let hello = ServerMessage::Hello(Hello {
        proto_version: PROTO_VERSION,
        name: app.cfg.name.clone(),
        model: app.cfg.model.clone(),
        version: crate::state::VERSION.into(),
        uptime_secs: app.started.elapsed().as_secs(),
    });
    send_json(&mut socket, &hello).await?;
    let snapshot = ServerMessage::State(app.snapshot().await);
    send_json(&mut socket, &snapshot).await?;

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
