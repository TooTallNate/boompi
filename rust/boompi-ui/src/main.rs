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

slint::include_modules!();

use boompi_proto::{decode_visualizer_frame, ClientMessage, PlaybackStatus, ServerMessage, Track};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

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

    // UI callbacks → outgoing command channel.
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

    // Networking runs on its own thread with a small tokio runtime; UI
    // updates hop back to the Slint event loop via `upgrade_in_event_loop`.
    let weak = ui.as_weak();
    let backend = cli.backend.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(network_loop(backend, weak, rx));
    });

    ui.run()?;
    Ok(())
}

/// Connect (and reconnect forever) to the backend.
async fn network_loop(
    url: String,
    weak: Weak<AppWindow>,
    mut rx: mpsc::UnboundedReceiver<ClientMessage>,
) {
    loop {
        set_connected(&weak, false);
        match connect_async(url.as_str()).await {
            Ok((stream, _)) => {
                eprintln!("connected to {url}");
                set_connected(&weak, true);
                session(stream, &weak, &mut rx).await;
                eprintln!("disconnected from {url}");
            }
            Err(err) => eprintln!("connect {url}: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn session(
    stream: WsStream,
    weak: &Weak<AppWindow>,
    rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
) {
    let (mut sink, mut source) = stream.split();
    loop {
        tokio::select! {
            incoming = source.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(msg) => apply_server_message(weak, msg),
                        Err(err) => eprintln!("unparseable server message: {err}"),
                    }
                }
                Some(Ok(Message::Binary(data))) => {
                    if let Some(bars) = decode_visualizer_frame(&data) {
                        let bars: Vec<f32> = bars
                            .iter()
                            .map(|&b| b as f32 / u16::MAX as f32)
                            .collect();
                        let _ = weak.upgrade_in_event_loop(move |ui| {
                            ui.set_bars(ModelRc::new(VecModel::from(bars)));
                        });
                    }
                }
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

fn apply_server_message(weak: &Weak<AppWindow>, msg: ServerMessage) {
    let _ = weak.upgrade_in_event_loop(move |ui| match msg {
        ServerMessage::Hello(hello) => {
            ui.set_speaker_name(hello.name.into());
        }
        ServerMessage::State(state) => {
            ui.set_volume(state.volume);
            match &state.track {
                Some(track) => apply_track(&ui, track),
                None => clear_track(&ui),
            }
            if state.source.active.is_none() {
                clear_track(&ui);
            }
        }
        ServerMessage::Track(track) => apply_track(&ui, &track),
        ServerMessage::Source(source) => {
            if source.active.is_none() {
                clear_track(&ui);
            }
        }
        ServerMessage::Volume { level } => ui.set_volume(level),
        // Rendered by dedicated screens in Phases 2/3/5.
        ServerMessage::Battery(_)
        | ServerMessage::Pairing(_)
        | ServerMessage::Settings(_)
        | ServerMessage::Setup(_) => {}
    });
}

fn apply_track(ui: &AppWindow, track: &Track) {
    ui.set_has_track(true);
    ui.set_track_title(track.title.clone().unwrap_or_default().into());
    ui.set_track_artist(track.artist.clone().unwrap_or_default().into());
    ui.set_track_album(track.album.clone().unwrap_or_default().into());
    ui.set_playing(track.status == PlaybackStatus::Playing);
}

fn clear_track(ui: &AppWindow) {
    ui.set_has_track(false);
    ui.set_track_title("".into());
    ui.set_track_artist("".into());
    ui.set_track_album("".into());
    ui.set_playing(false);
}

fn set_connected(weak: &Weak<AppWindow>, connected: bool) {
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_connected(connected);
        if !connected {
            clear_track(&ui);
            ui.set_bars(ModelRc::new(VecModel::from(Vec::<f32>::new())));
        }
    });
}
