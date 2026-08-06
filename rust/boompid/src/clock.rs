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
