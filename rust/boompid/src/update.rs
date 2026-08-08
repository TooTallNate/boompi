//! OS software updates from GitHub Releases.
//!
//! Stable channel: the latest tagged release (vX.Y.Z). Edge channel:
//! the rolling "edge" prerelease that CI replaces on every green build
//! of the dev branch. Asset names and formats are a contract with the
//! release workflows (see .github/workflows/release.yml):
//!
//!   boompi-<board>-rootfs.ext4.zst
//!   boompi-<board>-boot-{a,b}.vfat.zst
//!   SHA256SUMS.txt      (uncompressed payload hashes)
//!   boompi-version.txt  (the /etc/boompi-version stamp)
//!
//! Applying streams each asset straight into the inactive slot's
//! partitions - /data (512MB) cannot stage a ~640MB bundle and tmpfs
//! is half of 1GB on the Pi 3 - hashing the decompressed stream on the
//! way through, then re-reads the partition to verify the media, and
//! finally arms the A/B trial boot (boompi-trial-boot: one-shot
//! PM_RSTS partition request on the Pi 3, kexec on the Pi 4; a crash
//! or power-cycle during the trial falls back to the running slot).
#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use boompi_proto::{ServerMessage, UpdateAction, UpdateChannel, UpdateState};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::state::SharedApp;

const REPO: &str = "TooTallNate/boompi";
const MARKER: &str = "/data/boompi-trial";

// ---------------------------------------------------------------------------
// GitHub Releases API (anonymous; 60 req/h per IP is plenty)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // GitHub rejects requests without a User-Agent.
        .user_agent(concat!("boompid/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()?)
}

async fn fetch_release(channel: UpdateChannel) -> Result<Release> {
    let url = match channel {
        UpdateChannel::Stable => format!("https://api.github.com/repos/{REPO}/releases/latest"),
        UpdateChannel::Edge => format!("https://api.github.com/repos/{REPO}/releases/tags/edge"),
    };
    let resp = client()?.get(&url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("no release published on this channel yet");
    }
    let text = resp.error_for_status()?.text().await?;
    Ok(serde_json::from_str(&text).context("parsing release JSON")?)
}

fn asset<'a>(rel: &'a Release, name: &str) -> Result<&'a Asset> {
    rel.assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| anyhow!("release {} has no asset {name}", rel.tag_name))
}

/// The candidate's version: the tag for stable releases, the
/// boompi-version.txt stamp for the moving "edge" tag.
async fn release_version(rel: &Release, channel: UpdateChannel) -> Result<String> {
    match channel {
        UpdateChannel::Stable => Ok(rel.tag_name.clone()),
        UpdateChannel::Edge => {
            let a = asset(rel, "boompi-version.txt")?;
            let text = client()?
                .get(&a.browser_download_url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Ok(text.trim().to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Version comparison
// ---------------------------------------------------------------------------

/// Parse "vX.Y.Z" or "vX.Y.Z-<sha>" into (X, Y, Z).
fn base_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.strip_prefix('v')?;
    let base = v.split('-').next()?;
    let mut it = base.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next()?.parse().ok()?;
    let pat = it.next()?.parse().ok()?;
    Some((maj, min, pat))
}

/// Should `candidate` be offered to a box running `current`?
///
/// Stable: only strictly newer base versions. A box on "v2.1.0-abc"
/// (an edge build AFTER the v2.1.0 bump) must not be offered "v2.1.0"
/// - version bumps only happen on the release commit itself, so a
/// suffixed stamp is always at or past its base release.
///
/// Edge: any different stamp (edge only moves forward). "dev" (local
/// builds, no /etc/boompi-version) takes anything.
fn is_upgrade(current: &str, candidate: &str, channel: UpdateChannel) -> bool {
    if current == candidate {
        return false;
    }
    if current == "dev" {
        return true;
    }
    match channel {
        UpdateChannel::Stable => match (base_version(current), base_version(candidate)) {
            (Some(cur), Some(cand)) => cand > cur,
            _ => current != candidate,
        },
        UpdateChannel::Edge => true,
    }
}

// ---------------------------------------------------------------------------
// Board / slot discovery
// ---------------------------------------------------------------------------

fn board() -> Result<&'static str> {
    let model = std::fs::read_to_string("/proc/device-tree/model").unwrap_or_default();
    if model.contains("Raspberry Pi 3") {
        Ok("pi3")
    } else if model.contains("Raspberry Pi 4") {
        Ok("pi4")
    } else {
        bail!("unsupported board: {model:?}");
    }
}

struct Slot {
    target_root: &'static str,
    target_boot: &'static str,
    /// The boot image file name for the target slot ("boot-a.vfat" /
    /// "boot-b.vfat").
    boot_file: &'static str,
}

fn inactive_slot() -> Result<Slot> {
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    if cmdline.contains("root=/dev/mmcblk0p3") {
        Ok(Slot {
            target_root: "/dev/mmcblk0p5",
            target_boot: "/dev/mmcblk0p2",
            boot_file: "boot-b.vfat",
        })
    } else if cmdline.contains("root=/dev/mmcblk0p5") {
        Ok(Slot {
            target_root: "/dev/mmcblk0p3",
            target_boot: "/dev/mmcblk0p1",
            boot_file: "boot-a.vfat",
        })
    } else {
        bail!("cannot determine active slot from /proc/cmdline");
    }
}

// ---------------------------------------------------------------------------
// State projection + ws plumbing
// ---------------------------------------------------------------------------

pub async fn state(app: &SharedApp) -> UpdateState {
    let s = app.shared.read().await;
    UpdateState {
        version: crate::state::os_version().to_string(),
        available: s.update_available.clone(),
        checking: s.update_checking,
        applying: s.update_applying.clone(),
        progress: s.update_progress,
        error: s.update_error.clone(),
    }
}

async fn broadcast_state(app: &SharedApp) {
    let snapshot = state(app).await;
    app.broadcast(ServerMessage::Update(snapshot));
}

/// Shared handler for the ws message (and /api/command).
pub async fn perform(app: &SharedApp, action: UpdateAction) -> Result<()> {
    match action {
        UpdateAction::Check => {
            {
                let mut s = app.shared.write().await;
                if s.update_checking {
                    return Ok(());
                }
                s.update_checking = true;
                s.update_error = None;
            }
            broadcast_state(app).await;
            let app = app.clone();
            tokio::spawn(async move {
                let result = check(&app).await;
                {
                    let mut s = app.shared.write().await;
                    s.update_checking = false;
                    if let Err(err) = &result {
                        tracing::warn!(%err, "update check failed");
                        s.update_error = Some(err.to_string());
                    }
                }
                broadcast_state(&app).await;
            });
        }
        UpdateAction::Apply => {
            let version = {
                let mut s = app.shared.write().await;
                if s.update_applying.is_some() {
                    bail!("an update is already in progress");
                }
                let Some(v) = s.update_available.clone() else {
                    bail!("no update available (check first)");
                };
                s.update_applying = Some(v.clone());
                s.update_progress = Some(0.0);
                s.update_error = None;
                v
            };
            if std::path::Path::new(MARKER).exists() {
                let mut s = app.shared.write().await;
                s.update_applying = None;
                s.update_progress = None;
                drop(s);
                broadcast_state(app).await;
                bail!("a previous update trial is still pending; reboot first");
            }
            broadcast_state(app).await;
            let app = app.clone();
            tokio::spawn(async move {
                match apply(&app, &version).await {
                    // On success the box is rebooting into the trial;
                    // leave `applying` up so the UIs keep showing it
                    // until the connection drops.
                    Ok(()) => tracing::info!(%version, "update staged; trial boot armed"),
                    Err(err) => {
                        tracing::warn!(%err, %version, "update failed");
                        let mut s = app.shared.write().await;
                        s.update_applying = None;
                        s.update_progress = None;
                        s.update_error = Some(err.to_string());
                        drop(s);
                        broadcast_state(&app).await;
                    }
                }
            });
        }
    }
    Ok(())
}

async fn check(app: &SharedApp) -> Result<()> {
    let channel = app.shared.read().await.settings.update_channel;
    let rel = fetch_release(channel).await?;
    let candidate = release_version(&rel, channel).await?;
    let current = crate::state::os_version();
    let available = is_upgrade(current, &candidate, channel).then_some(candidate.clone());
    tracing::info!(%current, %candidate, ?channel, offered = available.is_some(), "update check");
    app.shared.write().await.update_available = available;
    Ok(())
}

/// Periodic background check (spawned from main): once shortly after
/// boot, then every 6 hours.
pub async fn periodic(app: SharedApp) {
    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
    loop {
        {
            let mut s = app.shared.write().await;
            s.update_checking = true;
        }
        broadcast_state(&app).await;
        let result = check(&app).await;
        {
            let mut s = app.shared.write().await;
            s.update_checking = false;
            // Background checks only surface results, not errors - a
            // box with no internet shouldn't show a permanent error
            // banner in settings.
            if let Err(err) = result {
                tracing::debug!(%err, "periodic update check failed");
            }
        }
        broadcast_state(&app).await;
        tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
    }
}

// ---------------------------------------------------------------------------
// Apply: stream assets into the inactive slot
// ---------------------------------------------------------------------------

/// Progress layout across the whole apply: rootfs is ~90% of the bytes.
const P_ROOTFS_DL: (f32, f32) = (0.0, 0.68);
const P_ROOTFS_VERIFY: (f32, f32) = (0.68, 0.82);
const P_BOOT_DL: (f32, f32) = (0.82, 0.90);
const P_BOOT_VERIFY: (f32, f32) = (0.90, 0.98);

async fn apply(app: &SharedApp, version: &str) -> Result<()> {
    let channel = app.shared.read().await.settings.update_channel;
    let board = board()?;
    let slot = inactive_slot()?;

    // Re-fetch the release: asset URLs are not stored across the
    // check/apply gap (the edge release may even have been replaced -
    // in that case the version stamp check below catches it).
    let rel = fetch_release(channel).await?;
    let now = release_version(&rel, channel).await?;
    if now != version {
        bail!("release changed since the check (was {version}, now {now}); check again");
    }

    // SHA256SUMS.txt: "<sha256>  boompi-<board>-<file>" lines,
    // uncompressed payload hashes.
    let sums_url = &asset(&rel, "SHA256SUMS.txt")?.browser_download_url;
    let sums = client()?
        .get(sums_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let sum_for = |name: &str| -> Result<String> {
        sums.lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                Some((it.next()?.to_string(), it.next()?.to_string()))
            })
            .find(|(_, n)| n == name)
            .map(|(h, _)| h)
            .ok_or_else(|| anyhow!("SHA256SUMS.txt has no entry for {name}"))
    };

    let rootfs_name = format!("boompi-{board}-rootfs.ext4");
    let boot_name = format!("boompi-{board}-{}", slot.boot_file);

    let rootfs_asset = asset(&rel, &format!("{rootfs_name}.zst"))?;
    let boot_asset = asset(&rel, &format!("{boot_name}.zst"))?;
    let rootfs_sum = sum_for(&rootfs_name)?;
    let boot_sum = sum_for(&boot_name)?;

    tracing::info!(
        %version, board, target_root = slot.target_root, target_boot = slot.target_boot,
        "applying update"
    );

    let n = stream_to_device(
        app,
        rootfs_asset,
        slot.target_root,
        &rootfs_sum,
        P_ROOTFS_DL,
    )
    .await?;
    verify_device(app, slot.target_root, n, &rootfs_sum, P_ROOTFS_VERIFY).await?;
    let n = stream_to_device(app, boot_asset, slot.target_boot, &boot_sum, P_BOOT_DL).await?;
    verify_device(app, slot.target_boot, n, &boot_sum, P_BOOT_VERIFY).await?;

    set_progress(app, 1.0).await;

    // Hand off to the shared trial-boot script: restores autoboot.txt
    // on the candidate boot partition, records the trial marker and
    // arms the board's trial mechanism, ending in a reboot that takes
    // this daemon down with it.
    let out = tokio::process::Command::new("boompi-trial-boot")
        .arg(slot.target_root)
        .output()
        .await
        .context("running boompi-trial-boot")?;
    if !out.status.success() {
        bail!(
            "boompi-trial-boot failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

async fn set_progress(app: &SharedApp, p: f32) {
    let significant = {
        let mut s = app.shared.write().await;
        let prev = s.update_progress.unwrap_or(0.0);
        let significant = p - prev >= 0.02 || p >= 1.0;
        if significant {
            s.update_progress = Some(p);
        }
        significant
    };
    if significant {
        broadcast_state(app).await;
    }
}

/// Download `asset`, decompress the zstd stream, and write it straight
/// to `dev`, hashing the decompressed bytes on the way through.
/// Returns the number of decompressed bytes written.
async fn stream_to_device(
    app: &SharedApp,
    asset: &Asset,
    dev: &str,
    expected_sha: &str,
    range: (f32, f32),
) -> Result<u64> {
    let resp = client()?
        .get(&asset.browser_download_url)
        .send()
        .await?
        .error_for_status()?;

    // Progress tracks COMPRESSED bytes off the wire (that's the slow
    // part and the denominator we know: the asset size).
    let fetched = Arc::new(AtomicU64::new(0));
    let counter = fetched.clone();
    let stream = resp
        .bytes_stream()
        .inspect_ok(move |chunk| {
            counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        })
        .map_err(std::io::Error::other);
    let reader = tokio_util::io::StreamReader::new(stream);
    let mut decoder = async_compression::tokio::bufread::ZstdDecoder::new(reader);

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(dev)
        .await
        .with_context(|| format!("opening {dev}"))?;

    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = decoder.read(&mut buf).await.context("download stream")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .await
            .with_context(|| format!("writing {dev}"))?;
        written += n as u64;
        let frac = fetched.load(Ordering::Relaxed) as f32 / asset.size.max(1) as f32;
        set_progress(app, range.0 + (range.1 - range.0) * frac.min(1.0)).await;
    }
    file.sync_all().await?;

    let got = format!("{:x}", hasher.finalize());
    if got != expected_sha {
        bail!("download of {} corrupt (sha256 mismatch)", asset.name);
    }
    Ok(written)
}

/// Re-read `len` bytes from the device and verify the media matches
/// what was hashed on the way in.
async fn verify_device(
    app: &SharedApp,
    dev: &str,
    len: u64,
    expected_sha: &str,
    range: (f32, f32),
) -> Result<()> {
    let mut file = tokio::fs::File::open(dev)
        .await
        .with_context(|| format!("opening {dev} for verify"))?;
    let mut hasher = Sha256::new();
    let mut remaining = len;
    let mut buf = vec![0u8; 1 << 20];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..want]).await?;
        if n == 0 {
            bail!("{dev} shorter than the written image");
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
        let frac = (len - remaining) as f32 / len.max(1) as f32;
        set_progress(app, range.0 + (range.1 - range.0) * frac).await;
    }
    let got = format!("{:x}", hasher.finalize());
    if got != expected_sha {
        bail!("verify FAILED: {dev} does not match the downloaded image");
    }
    tracing::info!(dev, len, "slot verified (sha256)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_rules() {
        use UpdateChannel::*;
        // stable: strictly newer base versions only
        assert!(is_upgrade("v2.0.0", "v2.0.1", Stable));
        assert!(is_upgrade("v2.0.0-abc1234", "v2.0.1", Stable));
        assert!(!is_upgrade("v2.0.1", "v2.0.1", Stable));
        // post-release edge build must not be "upgraded" to its own base
        assert!(!is_upgrade("v2.0.1-abc1234", "v2.0.1", Stable));
        assert!(!is_upgrade("v2.1.0", "v2.0.9", Stable));
        assert!(is_upgrade("v2.9.9", "v3.0.0", Stable));
        // edge: any different stamp
        assert!(is_upgrade("v2.0.1-abc1234", "v2.0.1-def5678", Edge));
        assert!(!is_upgrade("v2.0.1-abc1234", "v2.0.1-abc1234", Edge));
        assert!(is_upgrade("v2.0.1", "v2.0.1-def5678", Edge));
        // dev boxes take anything
        assert!(is_upgrade("dev", "v2.0.0", Stable));
        assert!(is_upgrade("dev", "v2.0.0-abc1234", Edge));
    }
}
