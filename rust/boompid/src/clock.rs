//! System clock/timezone via systemd's `org.freedesktop.timedate1`.
//!
//! Reads work everywhere; writes need privilege (root on the appliance).
//! On the dev box (boompid as `pi`, non-interactive) polkit denies
//! SetTimezone/SetNTP - the error is surfaced to the caller rather than
//! prompting, since there is nobody to prompt on an appliance.

#![cfg(target_os = "linux")]

use serde::Serialize;

#[zbus::proxy(
    interface = "org.freedesktop.timedate1",
    default_service = "org.freedesktop.timedate1",
    default_path = "/org/freedesktop/timedate1"
)]
trait TimeDate1 {
    #[zbus(property)]
    fn timezone(&self) -> zbus::Result<String>;
    #[zbus(property, name = "NTP")]
    fn ntp(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "NTPSynchronized")]
    fn ntp_synchronized(&self) -> zbus::Result<bool>;
    fn list_timezones(&self) -> zbus::Result<Vec<String>>;
    fn set_timezone(&self, timezone: &str, interactive: bool) -> zbus::Result<()>;
    #[zbus(name = "SetNTP")]
    fn set_ntp(&self, use_ntp: bool, interactive: bool) -> zbus::Result<()>;
}

#[derive(Debug, Serialize)]
pub struct ClockStatus {
    pub timezone: String,
    pub ntp: bool,
    pub synchronized: bool,
    /// Current unix time in ms - lets clients display device time and
    /// compute a local offset.
    pub now_ms: u64,
    pub timezones: Vec<String>,
}

pub async fn status() -> anyhow::Result<ClockStatus> {
    let conn = zbus::Connection::system().await?;
    let td = TimeDate1Proxy::new(&conn).await?;
    Ok(ClockStatus {
        timezone: td.timezone().await.unwrap_or_default(),
        ntp: td.ntp().await.unwrap_or(false),
        synchronized: td.ntp_synchronized().await.unwrap_or(false),
        now_ms: crate::state::now_ms(),
        timezones: td.list_timezones().await.unwrap_or_default(),
    })
}

/// Re-apply the persisted timezone/NTP prefs (best-effort). /etc lives
/// on the A/B rootfs, so an OTA silently resets the system copy to the
/// image default; the durable copy is in /data (boompi.toml).
pub async fn restore(app: &crate::state::SharedApp) {
    let (tz, ntp) = {
        let s = app.shared.read().await;
        (s.timezone.clone(), s.ntp)
    };
    if tz.is_none() && ntp.is_none() {
        return;
    }
    match set(tz.as_deref(), ntp).await {
        Ok(()) => tracing::info!(?tz, ?ntp, "restored persisted clock prefs"),
        Err(err) => tracing::warn!(%err, "failed to restore persisted clock prefs"),
    }
}

/// Unix seconds of the last moment the clock was verified good by a
/// client offer (stepped, or already within tolerance). 0 = never.
static LAST_VERIFIED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The clock was stepped or confirmed accurate recently - fallback
/// time sources (the CTS read, which can prompt for pairing) should
/// stand down.
pub fn recently_verified() -> bool {
    let last = LAST_VERIFIED.load(std::sync::atomic::Ordering::Relaxed);
    last != 0 && crate::state::now_ms() / 1000 - last < 600
}

/// Whether the kernel clock has been NTP-disciplined this boot
/// (timesyncd via timedate1). Errors read as "not synchronized" so
/// fallback time sources stay available when D-Bus is unhappy.
pub async fn ntp_synchronized() -> bool {
    async {
        let conn = zbus::Connection::system().await.ok()?;
        let td = TimeDate1Proxy::new(&conn).await.ok()?;
        td.ntp_synchronized().await.ok()
    }
    .await
    .unwrap_or(false)
}

/// Fallback clock sync from a connected client (browser `Date.now()`,
/// phone app over BLE). The boxes have no RTC, so when NTP is
/// unreachable the clock is off by months; any client that connects
/// knows the time better than we do.
///
/// NTP stays authoritative: if timesyncd reports a successful sync the
/// offer is ignored, and if NTP comes back later it simply overwrites
/// whatever a client set. Steps the clock directly via
/// `clock_settime(2)` because timedate1's `SetTime` refuses while NTP
/// is *enabled*, and we want NTP left enabled so it can win later.
///
/// Returns `true` if the clock was stepped.
pub async fn offer_time(epoch_ms: u64) -> anyhow::Result<bool> {
    // Plausibility window: rejects zero/garbage and 32-bit-ish
    // overflow artifacts from buggy clients.
    const MIN_MS: u64 = 1_704_067_200_000; // 2024-01-01
    const MAX_MS: u64 = 4_102_444_800_000; // 2100-01-01
    if !(MIN_MS..MAX_MS).contains(&epoch_ms) {
        anyhow::bail!("implausible epoch_ms {epoch_ms}");
    }

    // NTP synced -> the system clock is already better than any client's.
    if ntp_synchronized().await {
        return Ok(false);
    }

    // Already close enough (e.g. a second client reconnecting moments
    // after the first synced us): don't churn the clock.
    let now = crate::state::now_ms();
    let delta_ms = epoch_ms.abs_diff(now);
    if delta_ms < 5_000 {
        mark_verified();
        return Ok(false);
    }

    let ts = libc::timespec {
        tv_sec: (epoch_ms / 1000) as libc::time_t,
        tv_nsec: ((epoch_ms % 1000) * 1_000_000) as libc::c_long,
    };
    // SAFETY: plain syscall with a valid timespec; needs CAP_SYS_TIME
    // (root on the appliance) - EPERM on the dev box surfaces as Err.
    if unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) } != 0 {
        return Err(anyhow::anyhow!(std::io::Error::last_os_error()).context("clock_settime"));
    }
    tracing::info!(
        from_ms = now,
        to_ms = epoch_ms,
        delta_ms,
        "clock stepped from client-offered time (NTP not synchronized)"
    );
    mark_verified();
    Ok(true)
}

fn mark_verified() {
    LAST_VERIFIED.store(
        crate::state::now_ms() / 1000,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub async fn set(timezone: Option<&str>, ntp: Option<bool>) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let td = TimeDate1Proxy::new(&conn).await?;
    if let Some(tz) = timezone {
        td.set_timezone(tz, false).await?;
        tracing::info!(%tz, "timezone changed");
    }
    if let Some(ntp) = ntp {
        td.set_ntp(ntp, false).await?;
        tracing::info!(ntp, "NTP toggled");
    }
    Ok(())
}
