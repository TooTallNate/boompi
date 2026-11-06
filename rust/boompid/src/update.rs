//! OS software updates from GitHub Releases.
//!
//! Stable channel: the latest tagged release (vX.Y.Z). Edge channel:
//! the rolling "edge" prerelease that CI replaces on every green build
//! of the dev branch. The update is one self-contained asset, a
//! contract with the release workflows (see .github/workflows/):
//!
//!   boompi-update.tar
//!     SHA256SUMS.txt      (uncompressed payload hashes; first)
//!     boompi-version.txt  (the /etc/boompi-version stamp)
//!     rootfs.ext4.zst
//!     boot-a.vfat.zst
//!     boot-b.vfat.zst
//!
//! The image is board-generic, so there is exactly one bundle; the
//! box streams the tar and routes the entries it needs (rootfs + its
//! inactive slot's boot image) straight onto the partitions - /data
//! (512MB) cannot stage a ~640MB bundle and tmpfs is half of 1GB on
//! the Pi 3 - hashing the decompressed stream on the way through,
//! skipping the other slot's boot image, then re-reading the
//! partitions to verify the media, and finally arming the A/B trial
//! boot (boompi-trial-boot: one-shot PM_RSTS partition request on the
//! Pi 3, autoboot flip with sick-rollback on the Pi 4).
//!
//! The edge release's version stamp lives in the release notes
//! ("stamp: vX.Y.Z-sha") so the release carries exactly two kinds of
//! assets: sdcard images for flashing and the update bundle.
#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use boompi_proto::{ServerMessage, UpdateAction, UpdateChannel, UpdateStage, UpdateState};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::state::SharedApp;
use crate::transfer::{BoxReader, OpenError, OpenFuture, RateMeter, ResumableReader, RetryPolicy};

const REPO: &str = "TooTallNate/boompi";
const MARKER: &str = "/data/boompi-trial";

// ---------------------------------------------------------------------------
// GitHub Releases API (anonymous; 60 req/h per IP is plenty)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
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

/// Opener for [`ResumableReader`]: GET the bundle, from `offset` via a
/// Range request when resuming. GitHub answers browser_download_url
/// with a redirect to a short-lived signed object URL; reqwest follows
/// it with the Range header intact, and every reopen starts from the
/// stable URL so an expired signature is never reused.
///
/// Resuming is only safe against the same object: the resumed
/// response must cover exactly `offset..` of a `size`-byte object
/// with the first response's ETag (when the server sends one).
fn bundle_opener(
    client: reqwest::Client,
    url: String,
    size: u64,
) -> impl FnMut(u64) -> OpenFuture + Send + Unpin {
    let etag: Arc<std::sync::Mutex<Option<String>>> = Arc::default();
    move |offset| {
        let (client, url, etag) = (client.clone(), url.clone(), etag.clone());
        Box::pin(async move {
            let fatal = |msg: String| OpenError::Fatal(std::io::Error::other(msg));
            let changed =
                || fatal("the update changed on the server while downloading; check again".into());
            let mut req = client.get(&url);
            if offset > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let resp = req
                .send()
                .await
                .map_err(|e| OpenError::Retry(std::io::Error::other(e)))?;
            let status = resp.status();
            if status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            {
                return Err(OpenError::Retry(std::io::Error::other(format!(
                    "HTTP {status} downloading the update"
                ))));
            }
            let header = |name: reqwest::header::HeaderName| {
                resp.headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            };
            let range = header(reqwest::header::CONTENT_RANGE)
                .map(|v| crate::transfer::parse_content_range(&v));
            let resp_etag = header(reqwest::header::ETAG);
            match status {
                reqwest::StatusCode::OK if offset == 0 => {
                    if resp.content_length().is_some_and(|n| n != size) {
                        return Err(changed());
                    }
                }
                reqwest::StatusCode::PARTIAL_CONTENT => match range {
                    Some(Some((_, _, Some(total)))) if total != size => return Err(changed()),
                    Some(Some((start, end, Some(_)))) if start == offset && end + 1 == size => {}
                    // Wrong window, or "bytes a-b/*" (object size unknown).
                    _ => return Err(fatal("server did not resume the download".into())),
                },
                s if offset > 0 && s == reqwest::StatusCode::OK => {
                    return Err(fatal("server did not resume the download".into()));
                }
                s => {
                    return Err(fatal(format!("HTTP {s} downloading the update")));
                }
            }
            {
                let mut known = etag.lock().unwrap();
                match (known.as_deref(), resp_etag) {
                    (Some(a), Some(b)) if a != b => return Err(changed()),
                    (None, Some(b)) if offset == 0 => *known = Some(b),
                    _ => {}
                }
            }
            let body = resp.bytes_stream().map_err(std::io::Error::other);
            Ok(Box::new(tokio_util::io::StreamReader::new(body)) as BoxReader)
        })
    }
}

/// The hosted remote (boompi.n8.io) proxies + caches the release
/// lookup: a fleet of 10-minute edge pollers costs GitHub roughly one
/// request per cache window instead of one per box. It's a cache, not
/// a dependency - any failure falls back to GitHub directly.
const RELEASE_PROXY: &str = "https://boompi.n8.io/api/release";

async fn fetch_release(channel: UpdateChannel) -> Result<Release> {
    let chan = match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Edge => "edge",
    };
    match fetch_release_from(&format!("{RELEASE_PROXY}?channel={chan}")).await {
        Ok(rel) => return Ok(rel),
        Err(err) => tracing::debug!(%err, "release proxy unavailable; asking GitHub directly"),
    }
    let url = match channel {
        UpdateChannel::Stable => format!("https://api.github.com/repos/{REPO}/releases/latest"),
        UpdateChannel::Edge => format!("https://api.github.com/repos/{REPO}/releases/tags/edge"),
    };
    fetch_release_from(&url).await
}

async fn fetch_release_from(url: &str) -> Result<Release> {
    // reqwest's Display hides the cause chain ("error sending request
    // for url (...)" - was it DNS? TLS? timeout?), and error_for_status
    // buries the code. Spell both out: these strings surface verbatim
    // in every settings UI.
    let resp = client()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("request to {url} failed"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!("no release published on this channel yet");
    }
    if !status.is_success() {
        bail!("HTTP {status} from {url}");
    }
    let text = resp.text().await.context("reading release response")?;
    Ok(serde_json::from_str(&text).context("parsing release JSON")?)
}

fn asset<'a>(rel: &'a Release, name: &str) -> Result<&'a Asset> {
    rel.assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| anyhow!("release {} has no asset {name}", rel.tag_name))
}

/// The candidate's version: the tag for stable releases, the
/// "stamp: ..." line in the release notes for the moving "edge" tag
/// (the bundle carries the same stamp in boompi-version.txt, checked
/// again while applying).
async fn release_version(rel: &Release, channel: UpdateChannel) -> Result<String> {
    match channel {
        UpdateChannel::Stable => Ok(rel.tag_name.clone()),
        UpdateChannel::Edge => rel
            .body
            .lines()
            .find_map(|l| l.trim_start().strip_prefix("stamp:"))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow!("edge release notes carry no stamp line")),
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
        stage: s.update_stage,
        progress: s.update_progress,
        error: s.update_error.clone(),
        downloaded_bytes: s.update_downloaded,
        total_bytes: s.update_total,
        bytes_per_second: s.update_rate,
    }
}

fn clear_download(s: &mut crate::state::Shared) {
    s.update_downloaded = None;
    s.update_total = None;
    s.update_rate = None;
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
                        s.update_error = Some(format!("{err:#}"));
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
                clear_download(&mut s);
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
                        s.update_stage = None;
                        s.update_progress = None;
                        clear_download(&mut s);
                        s.update_error = Some(format!("{err:#}"));
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
/// boot, then every 6 hours - or every 10 minutes on the edge
/// channel, where a green build lands with most pushes and the whole
/// point of opting in is riding the front of the wave.
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
        // Channel is a live setting: re-read each cycle so toggling
        // "bleeding edge" takes effect at the next wakeup, no restart.
        let edge = app.shared.read().await.settings.update_channel
            == boompi_proto::UpdateChannel::Edge;
        let secs = if edge { 10 * 60 } else { 6 * 60 * 60 };
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
}

// ---------------------------------------------------------------------------
// Apply: stream assets into the inactive slot
// ---------------------------------------------------------------------------

/// Progress layout across the whole apply: one streamed tarball, then
/// two partition re-read verifies.
const P_STREAM: (f32, f32) = (0.0, 0.80);
const P_ROOTFS_VERIFY: (f32, f32) = (0.80, 0.94);
const P_BOOT_VERIFY: (f32, f32) = (0.94, 0.98);

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

    let bundle = asset(&rel, "boompi-update.tar")?;
    tracing::info!(
        %version, board, target_root = slot.target_root, target_boot = slot.target_boot,
        "applying update (streaming {})", bundle.name
    );

    // One pass over the tar stream: manifest + stamp first (the
    // archive is written in that order), then payloads routed to their
    // partitions or skipped. Connection drops and stalls are resumed
    // underneath with HTTP Range requests, invisible to the tar/zstd/
    // sha pipeline. The per-payload sha256 stays the end-to-end check:
    // a mis-spliced or swapped object fails it.
    let delivered = Arc::new(AtomicU64::new(0));
    let body = ResumableReader::new(
        bundle.size,
        bundle_opener(client()?, bundle.browser_download_url.clone(), bundle.size),
        RetryPolicy::default(),
        delivered.clone(),
    );
    let mut tar = crate::tarstream::TarReader::new(body);
    let ticker = DownloadTicker::spawn(app, delivered, bundle.size);

    let boot_entry = format!("{}.zst", slot.boot_file);
    let mut sums: Option<String> = None;
    let mut rootfs: Option<(u64, String)> = None; // (bytes written, sha)
    let mut boot: Option<(u64, String)> = None;
    let sum_for = |sums: &Option<String>, name: &str| -> Result<String> {
        sums.as_deref()
            .context("bundle payloads precede SHA256SUMS.txt")?
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                Some((it.next()?.to_string(), it.next()?.to_string()))
            })
            .find(|(_, n)| n == name)
            .map(|(h, _)| h)
            .ok_or_else(|| anyhow!("SHA256SUMS.txt has no entry for {name}"))
    };

    set_stage(app, UpdateStage::DownloadingSystem).await;
    while let Some(entry) = tar.next_entry().await? {
        match entry.name.as_str() {
            "SHA256SUMS.txt" => {
                let raw = tar.read_entry(&entry, 1 << 20).await?;
                sums = Some(String::from_utf8_lossy(&raw).into_owned());
            }
            "boompi-version.txt" => {
                let raw = tar.read_entry(&entry, 4096).await?;
                let stamp = String::from_utf8_lossy(&raw).trim().to_string();
                if stamp != version {
                    bail!("bundle stamp {stamp} does not match the offered {version}");
                }
            }
            "rootfs.ext4.zst" => {
                let sha = sum_for(&sums, "rootfs.ext4")?;
                let n =
                    write_tar_entry_to_device(&mut tar, entry.size, slot.target_root, &sha).await?;
                rootfs = Some((n, sha));
                set_stage(app, UpdateStage::DownloadingBoot).await;
            }
            name if name == boot_entry => {
                let plain = name.trim_end_matches(".zst");
                let sha = sum_for(&sums, plain)?;
                let n =
                    write_tar_entry_to_device(&mut tar, entry.size, slot.target_boot, &sha).await?;
                boot = Some((n, sha));
            }
            _ => tar.skip_entry(entry.size).await?, // the other slot's boot image
        }
    }
    drop(tar);
    drop(ticker);
    let (rootfs_len, rootfs_sum) = rootfs.context("bundle carried no rootfs.ext4.zst")?;
    let (boot_len, boot_sum) = boot.with_context(|| format!("bundle carried no {boot_entry}"))?;

    {
        let mut s = app.shared.write().await;
        clear_download(&mut s);
        s.update_progress = Some(P_STREAM.1);
    }
    set_stage(app, UpdateStage::VerifyingSystem).await;
    verify_device(
        app,
        slot.target_root,
        rootfs_len,
        &rootfs_sum,
        P_ROOTFS_VERIFY,
    )
    .await?;
    set_stage(app, UpdateStage::VerifyingBoot).await;
    verify_device(app, slot.target_boot, boot_len, &boot_sum, P_BOOT_VERIFY).await?;

    // The bundle's boot image is board-generic: re-materialize this
    // box's firmware config (display/rotation/wiring fragment from
    // /data/box/config.txt) into the freshly written partition. A
    // failure aborts the update *before* the trial is armed - booting
    // a candidate without its box profile could look healthy to the
    // sick-check (boompid runs fine) while the panel stays dark.
    let out = tokio::process::Command::new("boompi-apply-box-config")
        .arg(slot.target_boot)
        .output()
        .await
        .context("running boompi-apply-box-config")?;
    if !out.status.success() {
        bail!(
            "boompi-apply-box-config failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    set_stage(app, UpdateStage::Restarting).await;
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

async fn set_stage(app: &SharedApp, stage: UpdateStage) {
    app.shared.write().await.update_stage = Some(stage);
    broadcast_state(app).await;
}

async fn set_progress(app: &SharedApp, p: f32) {
    let significant = {
        let mut s = app.shared.write().await;
        let prev = s.update_progress.unwrap_or(0.0);
        let significant = p - prev >= 0.01 || p >= 1.0;
        if significant {
            s.update_progress = Some(p);
        }
        significant
    };
    if significant {
        broadcast_state(app).await;
    }
}

/// Download-phase progress: once a second, publish the compressed
/// bytes received off the wire (the slow part, and the denominator we
/// know: the whole bundle's size) mapped across [`P_STREAM`], plus
/// the byte counts and smoothed speed. Ticking on a clock instead of
/// per write keeps progress (and the speed readout dropping to 0)
/// live through stalls, reconnects and skipped entries.
///
/// Dropping the ticker stops it, so every early `?` exit from
/// `apply` cleans it up; `perform` then clears the fields.
struct DownloadTicker {
    handle: tokio::task::JoinHandle<()>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}

impl DownloadTicker {
    fn spawn(app: &SharedApp, delivered: Arc<AtomicU64>, total: u64) -> Self {
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app = app.clone();
        let flag = stopped.clone();
        let handle = tokio::spawn(async move {
            let mut meter = RateMeter::default();
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // The first tick has a single sample, so no rate yet: report
            // none rather than a 0 that UIs would read as a stall.
            let mut first = true;
            loop {
                tick.tick().await;
                let now = std::time::Instant::now();
                let got = delivered.load(Ordering::Relaxed).min(total);
                meter.sample(now, got);
                let rate = (!first).then(|| meter.rate(now));
                first = false;
                {
                    let mut s = app.shared.write().await;
                    // Checked under the lock: once the ticker is dropped, a
                    // tick that was already in flight must not
                    // resurrect the fields the caller just cleared.
                    if flag.load(Ordering::SeqCst) {
                        return;
                    }
                    let frac = got as f32 / total.max(1) as f32;
                    s.update_progress = Some(P_STREAM.0 + (P_STREAM.1 - P_STREAM.0) * frac);
                    s.update_downloaded = Some(got);
                    s.update_total = Some(total);
                    s.update_rate = rate;
                }
                broadcast_state(&app).await;
            }
        });
        Self { handle, stopped }
    }
}

impl Drop for DownloadTicker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.handle.abort();
    }
}

/// Decompress one zstd tar entry and write it straight to `dev`,
/// hashing the decompressed bytes on the way through. Returns the
/// number of decompressed bytes written.
async fn write_tar_entry_to_device<R>(
    tar: &mut crate::tarstream::TarReader<R>,
    entry_size: u64,
    dev: &str,
    expected_sha: &str,
) -> Result<u64>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let body = tokio::io::AsyncReadExt::take(tar.body(), entry_size);
    let mut decoder =
        async_compression::tokio::bufread::ZstdDecoder::new(tokio::io::BufReader::new(body));

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(dev)
        .await
        .with_context(|| format!("opening {dev}"))?;

    // Partition capacity: slot sizes are frozen at flash time (512M
    // originally, 1024M after boompi-migrate-roots). The entry's
    // decompressed size is unknown up front (zstd), so guard inside
    // the write loop - a clear refusal beats dd-style ENOSPC halfway
    // into an overflow of the neighboring partition.
    use tokio::io::AsyncSeekExt;
    let capacity = file.seek(std::io::SeekFrom::End(0)).await?;
    file.seek(std::io::SeekFrom::Start(0)).await?;

    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = decoder.read(&mut buf).await.context("update stream")?;
        if n == 0 {
            break;
        }
        if written + n as u64 > capacity {
            bail!(
                "image for {dev} exceeds the partition ({capacity} bytes); \
                 root slots need boompi-migrate-roots before this update"
            );
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .await
            .with_context(|| format!("writing {dev}"))?;
        written += n as u64;
    }
    file.sync_all().await?;

    // Drain whatever the decoder left unread (a well-formed entry is
    // consumed exactly; guard against trailing bytes), then the tar
    // padding.
    let mut inner = decoder.into_inner().into_inner();
    let mut sink = [0u8; 8192];
    loop {
        let n = inner.read(&mut sink).await?;
        if n == 0 {
            break;
        }
    }
    tar.finish_entry(entry_size).await?;

    let got = format!("{:x}", hasher.finalize());
    if got != expected_sha {
        bail!("bundle entry for {dev} corrupt (sha256 mismatch)");
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
