//! boompid — the Boompi backend daemon.
//!
//! Bridges audio sources (Bluetooth/BlueZ, Spotify/librespot,
//! AirPlay/shairport-sync), PipeWire volume, INA260 battery telemetry, and an
//! FFT visualizer, exposing everything to UI clients over WebSocket + HTTP.
//! See `docs/PLAN.md` at the repository root.
//!
//! Current status: server + protocol + `--sim` mode. Hardware sources land
//! in Phase 1.

mod config;
mod server;
mod sim;
mod state;

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "boompid", version, about)]
struct Cli {
    /// Address for the WebSocket/HTTP server.
    #[arg(long, default_value = "0.0.0.0:3001")]
    listen: SocketAddr,

    /// Run with simulated sources/battery/visualizer (no hardware needed;
    /// works on any OS). Intended for UI development.
    #[arg(long)]
    sim: bool,

    /// Path to the device config TOML (default: built-in defaults).
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "boompid=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = config::load(cli.config.as_deref())?;
    tracing::info!(name = %cfg.name, "starting boompid v{}", state::VERSION);

    let app = state::App::new(cfg);

    if cli.sim {
        tracing::info!("simulation mode: fake sources, battery, and visualizer");
        sim::spawn(app.clone());
    } else {
        // TODO(Phase 1): BlueZ source, PipeWire volume/visualizer, INA260.
        tracing::warn!(
            "hardware sources are not implemented yet (Phase 1); \
             serving with no active sources — try --sim"
        );
    }

    server::serve(app, cli.listen).await
}
