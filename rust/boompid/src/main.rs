//! boompid - the Boompi backend daemon.
//!
//! Bridges audio sources (Bluetooth/BlueZ, Spotify/librespot,
//! AirPlay/shairport-sync), PipeWire volume, INA260 battery telemetry, and an
//! FFT visualizer, exposing everything to UI clients over WebSocket + HTTP.
//! See `docs/PLAN.md` at the repository root.
//!
//! Current status: server + protocol + `--sim` mode. Hardware sources land
//! in Phase 1.

#[cfg(target_os = "linux")]
mod airplay;
#[cfg(target_os = "linux")]
mod artwork;
#[cfg(target_os = "linux")]
mod audio;
#[cfg(target_os = "linux")]
mod battery;
#[cfg(target_os = "linux")]
mod bluetooth;
mod boxprofile;
#[cfg(target_os = "linux")]
mod bt_agent;
#[cfg(target_os = "linux")]
mod clock;
mod config;
mod fonts;
// DSP is platform-independent (unit-tested everywhere) but only consumed by
// the Linux-only visualizer.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod dsp;
mod mqtt;
mod server;
mod sim;
// SoC estimation is platform-independent (unit-tested everywhere) but only
// consumed by the Linux-only battery thread.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod soc;
// Streaming tar parsing is platform-independent (unit-tested
// everywhere) but only consumed by the Linux-only updater.
#[cfg(target_os = "linux")]
mod spotify;
mod state;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod tarstream;
mod update;
#[cfg(target_os = "linux")]
mod visualizer;
#[cfg(target_os = "linux")]
mod wifi;

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
    /// Settings changes persist back to this file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Read-only seed config used when --config doesn't exist yet
    /// (appliance: image-baked hardware facts seeding /data on first boot).
    #[arg(long)]
    config_seed: Option<PathBuf>,

    /// Box hardware profile merged over the config (per-build facts:
    /// battery wiring, panel DPI, amp GPIO). Survives updates and
    /// factory resets.
    #[arg(long)]
    hardware_profile: Option<PathBuf>,
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
    let cfg = config::load_layered(
        cli.config.as_deref(),
        cli.config_seed.as_deref(),
        cli.hardware_profile.as_deref(),
    )?;
    tracing::info!(name = %cfg.name, "starting boompid v{}", state::VERSION);

    let app = state::App::new(cfg, cli.config.clone());

    if cli.sim {
        tracing::info!("simulation mode: fake sources, battery, and visualizer");
        sim::spawn(app.clone());
    } else {
        #[cfg(target_os = "linux")]
        {
            tracing::info!("hardware mode: BlueZ source + INA260 battery + visualizer");
            bluetooth::spawn(app.clone());
            battery::spawn(app.clone());
            visualizer::spawn(app.clone());
            spotify::spawn(app.clone());
            airplay::spawn(app.clone());
            // Re-apply persisted clock prefs (an OTA resets /etc).
            {
                let app = app.clone();
                tokio::spawn(async move { clock::restore(&app).await });
            }
            // Emoji font choice: regenerate the fontconfig fragment and
            // fall back to the built-in if the chosen file vanished.
            {
                let app = app.clone();
                tokio::spawn(async move {
                    let chosen = app.shared.read().await.emoji_font.clone();
                    let effective = fonts::reconcile(&chosen);
                    if effective != chosen {
                        tracing::warn!(%chosen, %effective, "emoji font missing; falling back");
                        app.shared.write().await.emoji_font = effective.into();
                        app.persist_config().await;
                    }
                });
            }
            // Periodic OS update checks against the release channel.
            {
                let app = app.clone();
                tokio::spawn(update::periodic(app));
            }
            // Home Assistant integration (idles until a broker is
            // configured in settings).
            {
                let app = app.clone();
                tokio::spawn(mqtt::run(app));
            }
            // Seed the volume from the current system state.
            {
                let app = app.clone();
                tokio::spawn(async move {
                    match audio::get_system_volume().await {
                        Ok(level) => {
                            let mut s = app.shared.write().await;
                            s.volume = level;
                            s.sink_volume = level;
                        }
                        Err(err) => tracing::warn!(%err, "could not read system volume"),
                    }
                });
            }
            // First boot: broadcast the onboarding hotspot when nothing
            // else provides a way to reach the setup page.
            {
                let app = app.clone();
                tokio::spawn(async move {
                    if !app.shared.read().await.setup.required {
                        return;
                    }
                    // Give NetworkManager a moment to settle after boot.
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    match wifi::status(false).await {
                        Ok(st) if st.supported && st.connected.is_none() && !st.ap_active => {
                            let name = app.speaker_name().await;
                            if let Err(err) = wifi::start_ap(&name).await {
                                tracing::warn!(%err, "onboarding AP failed to start");
                            }
                        }
                        Ok(_) => tracing::info!("setup pending; network already available"),
                        Err(err) => tracing::warn!(%err, "wifi status unavailable for onboarding"),
                    }
                });
            }
            // TODO(Phase 1, next): visualizer via PipeWire monitor capture.
        }
        #[cfg(not(target_os = "linux"))]
        tracing::warn!("hardware sources are Linux-only; try --sim");
    }

    server::serve(app, cli.listen).await
}
