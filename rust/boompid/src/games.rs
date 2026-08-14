//! Games library + RetroArch orchestration.
//!
//! ROMs are user content under /data/games/roms/<system>/ (uploaded
//! via the web UI; nothing ships in the image). Launching hands the
//! display over: boompi-ui stops (one DRM master at a time),
//! RetroArch runs as a transient systemd unit (surviving boompid
//! restarts), and the panel returns when the game exits - via the
//! in-game menu (Start+Select), the web Stop button, or death.
//!
//! Audio flows through PipeWire like every other source, so music and
//! gameplay coexist; while an external source is active the game's
//! stream is ducked to `Settings::game_volume`.

use crate::state::SharedApp;
use anyhow::{bail, Context, Result};
use boompi_proto::{Game, GamesState, ServerMessage};
use std::path::PathBuf;

/// (system id, libretro core, launchable extensions)
pub const SYSTEMS: &[(&str, &str, &[&str])] = &[
    ("nes", "fceumm", &["nes"]),
    ("snes", "snes9x", &["sfc", "smc"]),
    ("gb", "gambatte", &["gb"]),
    ("gbc", "gambatte", &["gbc"]),
    ("gba", "mgba", &["gba"]),
    ("n64", "mupen64plus_next", &["n64", "z64", "v64"]),
    // PSX: .bin tracks ride along with their .cue and are uploadable
    // but not listed as launchable titles themselves.
    ("psx", "pcsx_rearmed", &["cue", "chd", "pbp", "iso"]),
];

/// Upload targets beyond ROMs ("bios" for pcsx etc.).
pub const BIOS_EXTENSIONS: &[&str] = &["bin", "rom"];
/// Upload size caps: generous for disc systems, sane for carts.
pub fn upload_cap(system: &str) -> u64 {
    match system {
        "psx" => 1 << 30,   // 1GB (CHD/cue+bin)
        "bios" => 32 << 20, // 32MB
        _ => 128 << 20,     // 128MB
    }
}

pub fn games_dir() -> PathBuf {
    std::env::var_os("BOOMPI_GAMES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/games"))
}

pub fn roms_dir() -> PathBuf {
    games_dir().join("roms")
}

fn system_extensions(system: &str) -> Option<&'static [&'static str]> {
    if system == "bios" {
        return Some(BIOS_EXTENSIONS);
    }
    SYSTEMS
        .iter()
        .find(|(id, _, _)| *id == system)
        .map(|(_, _, exts)| *exts)
}

/// Extensions accepted for upload (launchable + companions like the
/// PSX .bin behind a .cue).
pub fn upload_extension_ok(system: &str, name: &str) -> bool {
    let Some(ext) = name.rsplit('.').next().map(str::to_ascii_lowercase) else {
        return false;
    };
    match system {
        "bios" => BIOS_EXTENSIONS.contains(&ext.as_str()),
        "psx" => ["cue", "chd", "pbp", "iso", "bin", "img", "ccd", "sub"].contains(&ext.as_str()),
        _ => system_extensions(system).is_some_and(|e| e.contains(&ext.as_str())),
    }
}

/// A file name that cannot escape its directory or confuse a shell.
pub fn sanitize_file_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 255
        || name.starts_with('.')
        || name.contains(['/', '\\', '\0'])
    {
        return None;
    }
    Some(name.to_string())
}

pub fn scan() -> Vec<Game> {
    let mut out = Vec::new();
    for (system, _, exts) in SYSTEMS {
        let dir = roms_dir().join(system);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            // Dotfiles: macOS AppleDouble siblings (._Game.nes) arrive
            // over SMB despite the veto and must never list as games.
            if file.starts_with('.') {
                continue;
            }
            let Some(ext) = file.rsplit('.').next().map(str::to_ascii_lowercase) else {
                continue;
            };
            if !exts.contains(&ext.as_str()) {
                continue;
            }
            let name = file
                .strip_suffix(&format!(".{ext}"))
                .unwrap_or(&file)
                .to_string();
            out.push(Game {
                system: system.to_string(),
                file,
                name,
                size: meta.len(),
            });
        }
    }
    out.sort_by(|a, b| a.system.cmp(&b.system).then(a.name.cmp(&b.name)));
    out
}

/// A gamepad is connected (joydev nodes appear for evdev joysticks).
pub fn gamepad_connected() -> bool {
    std::fs::read_dir("/dev/input")
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("js"))
        })
        .unwrap_or(false)
}

fn storage() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let dir = games_dir();
        let path = std::ffi::CString::new(dir.to_string_lossy().as_bytes()).unwrap();
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } == 0 {
            let free = stat.f_bavail as u64 * stat.f_frsize as u64;
            let total = stat.f_blocks as u64 * stat.f_frsize as u64;
            return (free, total);
        }
    }
    (0, 0)
}

/// Rebuild the shared games state and broadcast if it changed.
pub async fn refresh(app: &SharedApp) {
    // statvfs needs the path to exist; first boot has no /data/games
    // until the first upload otherwise.
    let _ = std::fs::create_dir_all(roms_dir());
    let running = app.shared.read().await.game_running.clone();
    let (storage_free, storage_total) = storage();
    let state = GamesState {
        games: scan(),
        running,
        gamepad: gamepad_connected(),
        storage_free,
        storage_total,
    };
    let changed = {
        let mut s = app.shared.write().await;
        let changed = s.games != state;
        s.games = state.clone();
        changed
    };
    if changed {
        app.broadcast(ServerMessage::Games(state));
    }
}

// ---------------------------------------------------------------------------
// Launch orchestration (appliance only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod run {
    use super::*;

    const UNIT: &str = "boompi-game";
    /// Written at launch so a restarted boompid can adopt a running
    /// game (and so recovery knows the panel was stopped on purpose).
    const RUNNING_MARKER: &str = "/run/boompi-game.json";

    async fn unit_active() -> bool {
        tokio::process::Command::new("systemctl")
            .args(["is-active", "--quiet", UNIT])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    async fn systemctl(args: &[&str]) -> Result<()> {
        let out = tokio::process::Command::new("systemctl")
            .args(args)
            .output()
            .await?;
        if !out.status.success() {
            bail!(
                "systemctl {args:?}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// Translate the panel mount rotation into RetroArch's
    /// video_rotation. Sources of truth and conventions:
    /// - /data/box/env SLINT_KMS_ROTATION: degrees CLOCKWISE (slint's
    ///   Rotate90 is documented "rotate 90 to the right").
    /// - video_rotation: 90-degree COUNTER-clockwise steps (libretro's
    ///   retro_set_rotation convention).
    fn panel_rotation_steps() -> u32 {
        let Ok(env) = std::fs::read_to_string("/data/box/env") else {
            return 0;
        };
        for line in env.lines() {
            if let Some(v) = line.trim().strip_prefix("SLINT_KMS_ROTATION=") {
                if let Ok(deg) = v.trim().parse::<u32>() {
                    return ((360 - (deg % 360)) % 360) / 90;
                }
            }
        }
        0
    }

    pub async fn launch(app: &SharedApp, system: &str, file: &str) -> Result<()> {
        let (_, core, _) = SYSTEMS
            .iter()
            .find(|(id, _, _)| *id == system)
            .context("unknown system")?;
        let rom = roms_dir().join(system).join(file);
        if !rom.is_file() {
            bail!("no such game: {system}/{file}");
        }
        if !gamepad_connected() {
            bail!("no gamepad connected - pair or plug one in first");
        }
        if app.shared.read().await.game_running.is_some() || unit_active().await {
            bail!("a game is already running");
        }
        let core_path = format!("/usr/lib/libretro/{core}_libretro.so");
        if !std::path::Path::new(&core_path).exists() {
            bail!("core {core} is not installed in this image");
        }

        let key = format!("{system}/{file}");
        std::fs::create_dir_all(games_dir().join("saves")).ok();
        std::fs::create_dir_all(games_dir().join("states")).ok();
        let _ = std::fs::write(RUNNING_MARKER, &key);

        // Per-boot overrides that depend on box state (/etc is the
        // static baseline): the game must rotate with the panel.
        let append_cfg = "/run/boompi-game-retroarch.cfg";
        let rotation = panel_rotation_steps();
        std::fs::write(append_cfg, format!("video_rotation = \"{rotation}\"\n"))
            .context("write retroarch append config")?;

        tracing::info!(%key, core, rotation, "launching game; panel UI stops");
        systemctl(&["stop", "boompi-ui"]).await?;
        let out = tokio::process::Command::new("systemd-run")
            .args([
                &format!("--unit={UNIT}"),
                "--collect",
                "--property=Restart=no",
                "/usr/bin/retroarch",
                "-L",
                &core_path,
                "--config",
                "/etc/retroarch.cfg",
                "--appendconfig",
                append_cfg,
            ])
            .arg(&rom)
            .output()
            .await?;
        if !out.status.success() {
            let _ = std::fs::remove_file(RUNNING_MARKER);
            let _ = systemctl(&["start", "boompi-ui"]).await;
            bail!(
                "failed to start retroarch: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        app.shared.write().await.game_running = Some(key);
        refresh(app).await;
        spawn_monitor(app.clone());
        Ok(())
    }

    pub async fn stop() -> Result<()> {
        systemctl(&["stop", UNIT]).await
    }

    /// Watch the game unit; when it exits (menu quit, web stop,
    /// crash), bring the panel back and duck/unduck along the way.
    fn spawn_monitor(app: SharedApp) {
        tokio::spawn(async move {
            let mut last_duck: Option<f32> = None;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if !unit_active().await {
                    break;
                }
                // Ducking: music takes precedence over gameplay.
                let (source_active, volume) = {
                    let s = app.shared.read().await;
                    (s.source.active.is_some(), s.settings.game_volume)
                };
                let want = if source_active { volume } else { 1.0 };
                if last_duck != Some(want) {
                    if set_stream_volume("RetroArch", want).await {
                        last_duck = Some(want);
                    }
                }
            }
            tracing::info!("game exited; panel UI returns");
            let _ = std::fs::remove_file(RUNNING_MARKER);
            let _ = systemctl(&["start", "boompi-ui"]).await;
            app.shared.write().await.game_running = None;
            refresh(&app).await;
        });
    }

    /// boompid (re)started: adopt a running game, or repair a
    /// half-orchestrated state (panel stopped, no game - e.g. boompid
    /// died between the two systemctl calls).
    pub fn spawn_recovery(app: SharedApp) {
        tokio::spawn(async move {
            if unit_active().await {
                let key =
                    std::fs::read_to_string(RUNNING_MARKER).unwrap_or_else(|_| "unknown".into());
                tracing::info!(%key, "adopting running game after restart");
                app.shared.write().await.game_running = Some(key);
                spawn_monitor(app.clone());
            } else {
                let ui_active = tokio::process::Command::new("systemctl")
                    .args(["is-active", "--quiet", "boompi-ui"])
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(true);
                if !ui_active && std::path::Path::new(RUNNING_MARKER).exists() {
                    tracing::warn!("panel down with no game; recovering");
                    let _ = std::fs::remove_file(RUNNING_MARKER);
                    let _ = systemctl(&["start", "boompi-ui"]).await;
                }
            }
            refresh(&app).await;
            // Gamepad presence changes (pairing, unplug) with no event
            // source of their own: cheap periodic re-check.
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                refresh(&app).await;
            }
        });
    }

    /// Set a PipeWire stream's volume by application name (pw-dump to
    /// find the node, wpctl to set). Returns false when the node
    /// isn't up yet.
    async fn set_stream_volume(app_name: &str, volume: f32) -> bool {
        let Ok(out) = tokio::process::Command::new("pw-dump").output().await else {
            return false;
        };
        let Ok(json): Result<serde_json::Value, _> = serde_json::from_slice(&out.stdout) else {
            return false;
        };
        let Some(arr) = json.as_array() else {
            return false;
        };
        for obj in arr {
            let props = &obj["info"]["props"];
            if props["application.name"] == app_name
                && props["media.class"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("Stream/Output/Audio"))
            {
                if let Some(id) = obj["id"].as_u64() {
                    return tokio::process::Command::new("wpctl")
                        .args(["set-volume", &id.to_string(), &format!("{volume:.2}")])
                        .status()
                        .await
                        .map(|s| s.success())
                        .unwrap_or(false);
                }
            }
        }
        false
    }
}

#[cfg(target_os = "linux")]
pub use run::{launch, spawn_recovery, stop};

#[cfg(not(target_os = "linux"))]
pub async fn launch(_app: &SharedApp, _system: &str, _file: &str) -> Result<()> {
    bail!("games run on the appliance only")
}
#[cfg(not(target_os = "linux"))]
pub async fn stop() -> Result<()> {
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub fn spawn_recovery(app: SharedApp) {
    tokio::spawn(async move {
        refresh(&app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_allowlists() {
        assert!(upload_extension_ok("nes", "Mario.nes"));
        assert!(upload_extension_ok("nes", "MARIO.NES"));
        assert!(!upload_extension_ok("nes", "mario.sfc"));
        assert!(upload_extension_ok("psx", "game.cue"));
        assert!(upload_extension_ok("psx", "game.bin")); // cue companion
        assert!(!upload_extension_ok("gba", "notes.txt"));
        assert!(upload_extension_ok("bios", "scph1001.bin"));
    }

    #[test]
    fn file_name_sanitizing() {
        assert_eq!(
            sanitize_file_name("Mario.nes").as_deref(),
            Some("Mario.nes")
        );
        assert!(sanitize_file_name("../../etc/passwd").is_none());
        assert!(sanitize_file_name(".hidden").is_none());
        assert!(sanitize_file_name("a/b.nes").is_none());
        assert!(sanitize_file_name("").is_none());
    }

    #[test]
    fn scan_finds_roms() {
        let dir = std::env::temp_dir().join(format!("boompi-games-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("roms/nes")).unwrap();
        std::fs::create_dir_all(dir.join("roms/psx")).unwrap();
        std::fs::write(dir.join("roms/nes/Mario.nes"), b"x").unwrap();
        std::fs::write(dir.join("roms/psx/Game.cue"), b"x").unwrap();
        std::fs::write(dir.join("roms/psx/Game.bin"), b"x").unwrap(); // companion: not listed
        std::fs::write(dir.join("roms/nes/._Mario.nes"), b"x").unwrap(); // AppleDouble: skipped
        std::env::set_var("BOOMPI_GAMES_DIR", &dir);
        let games = scan();
        std::env::remove_var("BOOMPI_GAMES_DIR");
        std::fs::remove_dir_all(&dir).ok();
        let names: Vec<_> = games.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["Mario", "Game"]);
        assert_eq!(games[1].system, "psx");
    }
}
