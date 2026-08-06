//! Wi-Fi management via NetworkManager's `nmcli`.
//!
//! nmcli's terse mode (`-t`) is a stable scripting interface and saves us
//! a few hundred lines of NM D-Bus proxies. Requires the boompid user to
//! be allowed by polkit (root on the appliance; `netdev` group on the dev
//! box).
//!
//! AP mode: a `shared` connection named [`AP_CONNECTION`] turns wlan0 into
//! an open access point with NM's built-in DHCP - the onboarding path when
//! no Wi-Fi is configured (see docs/PLAN.md Phase 5).

#![cfg(target_os = "linux")]

use serde::Serialize;
use tokio::process::Command;

/// NM connection id (profile name) used for the onboarding access point.
pub const AP_CONNECTION: &str = "boompi-ap";

/// Last real scan results. A single radio can't scan while it hosts the
/// onboarding hotspot, so the captive-portal Wi-Fi step serves this cache
/// (refreshed just before the AP goes up) instead of an empty list.
static SCAN_CACHE: std::sync::Mutex<Vec<WifiNetwork>> = std::sync::Mutex::new(Vec::new());

#[derive(Debug, Default, Serialize)]
pub struct WifiStatus {
    /// A Wi-Fi capable device exists.
    pub supported: bool,
    /// Radio on?
    pub enabled: bool,
    /// SSID of the active connection, if any.
    pub connected: Option<String>,
    /// wlan IP (CIDR form) when connected or in AP mode.
    pub ip: Option<String>,
    /// True while the boompi-ap profile is active (onboarding hotspot).
    pub ap_active: bool,
    /// Scan results, deduped by SSID (strongest kept), sorted by signal.
    pub networks: Vec<WifiNetwork>,
    /// Saved Wi-Fi profile names (connectable without a password).
    pub saved: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    /// 0-100.
    pub signal: u8,
    /// Human security summary ("WPA2", "WPA2 WPA3", "" = open).
    pub security: String,
    pub in_use: bool,
    pub saved: bool,
}

async fn nmcli(args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("nmcli").args(args).output().await?;
    if !out.status.success() {
        anyhow::bail!(
            "nmcli {:?}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn status(scan: bool) -> anyhow::Result<WifiStatus> {
    let mut st = WifiStatus::default();

    // Radio + device presence.
    st.enabled = nmcli(&["-t", "radio", "wifi"]).await?.trim() == "enabled";
    let devices = nmcli(&["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev"]).await?;
    let mut wifi_dev: Option<(String, String, String)> = None; // dev, state, connection
    for line in devices.lines() {
        let f = split_terse(line);
        if f.len() >= 4 && f[1] == "wifi" {
            wifi_dev = Some((f[0].clone(), f[2].clone(), f[3].clone()));
            break;
        }
    }
    let Some((dev, state, connection)) = wifi_dev else {
        return Ok(st); // supported = false
    };
    st.supported = true;
    st.ap_active = connection == AP_CONNECTION;
    if state.starts_with("connected") && !st.ap_active && !connection.is_empty() {
        st.connected = Some(connection.clone());
    }
    if state.starts_with("connected") {
        if let Ok(out) = nmcli(&["-t", "-f", "IP4.ADDRESS", "dev", "show", &dev]).await {
            st.ip = out
                .lines()
                .next()
                .and_then(|l| split_terse(l).get(1).cloned())
                .filter(|s| !s.is_empty());
        }
    }

    // Saved Wi-Fi profiles (excluding our AP profile).
    let cons = nmcli(&["-t", "-f", "NAME,TYPE", "con", "show"]).await?;
    st.saved = cons
        .lines()
        .map(split_terse)
        .filter(|f| f.len() >= 2 && f[1] == "802-11-wireless" && f[0] != AP_CONNECTION)
        .map(|f| f[0].clone())
        .collect();

    // Scan (skipped in AP mode: leaving AP to scan would drop clients).
    if scan && st.enabled && !st.ap_active {
        let list = nmcli(&[
            "-t",
            "-f",
            "SSID,SIGNAL,SECURITY,IN-USE",
            "dev",
            "wifi",
            "list",
            "--rescan",
            "auto",
        ])
        .await?;
        let mut best: Vec<WifiNetwork> = Vec::new();
        for line in list.lines() {
            let f = split_terse(line);
            if f.len() < 4 || f[0].is_empty() {
                continue; // hidden SSID
            }
            let net = WifiNetwork {
                ssid: f[0].clone(),
                signal: f[1].parse().unwrap_or(0),
                security: f[2].replace("WPA1 ", "").clone(),
                in_use: f[3] == "*",
                saved: st.saved.iter().any(|s| s == &f[0]),
            };
            match best.iter_mut().find(|n| n.ssid == net.ssid) {
                Some(existing) => {
                    if net.signal > existing.signal || net.in_use {
                        *existing = net;
                    }
                }
                None => best.push(net),
            }
        }
        best.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
        *SCAN_CACHE.lock().unwrap() = best.clone();
        st.networks = best;
    } else if scan && st.enabled && st.ap_active {
        // Hotspot up: serve the pre-AP scan (nothing is "in use" - the
        // radio is busy being the hotspot).
        let mut cached = SCAN_CACHE.lock().unwrap().clone();
        for n in &mut cached {
            n.in_use = false;
        }
        st.networks = cached;
    }
    Ok(st)
}

/// Join a network. Saved profiles reconnect without a password; new ones
/// need `psk` unless the network is open.
pub async fn connect(ssid: &str, psk: Option<&str>) -> anyhow::Result<()> {
    // A saved profile with this name activates directly (keeps its psk).
    let saved = nmcli(&["-t", "-f", "NAME,TYPE", "con", "show"])
        .await?
        .lines()
        .map(split_terse)
        .any(|f| f.len() >= 2 && f[0] == ssid && f[1] == "802-11-wireless");
    if saved && psk.is_none() {
        nmcli(&["con", "up", "id", ssid]).await?;
    } else {
        let mut args = vec!["dev", "wifi", "connect", ssid];
        if let Some(psk) = psk {
            args.extend(["password", psk]);
        }
        nmcli(&args).await?;
    }
    tracing::info!(%ssid, "wifi connected");
    Ok(())
}

/// Delete a saved profile (disconnects if active).
pub async fn forget(name: &str) -> anyhow::Result<()> {
    nmcli(&["con", "delete", "id", name]).await?;
    tracing::info!(%name, "wifi profile forgotten");
    Ok(())
}

pub async fn set_radio(enabled: bool) -> anyhow::Result<()> {
    nmcli(&["radio", "wifi", if enabled { "on" } else { "off" }]).await?;
    tracing::info!(enabled, "wifi radio toggled");
    Ok(())
}

/// Bring up the onboarding access point (open network, NM shared IPv4 -
/// NM runs its own DHCP). Creates the profile on first use.
pub async fn start_ap(ssid: &str) -> anyhow::Result<()> {
    // Refresh the scan cache while the radio can still scan: the captive
    // portal's Wi-Fi step shows these networks (see SCAN_CACHE).
    if let Err(err) = status(true).await {
        tracing::debug!(%err, "pre-AP scan failed (continuing)");
    }
    let have_profile = nmcli(&["-t", "-f", "NAME", "con", "show"])
        .await?
        .lines()
        .any(|l| split_terse(l).first().map(String::as_str) == Some(AP_CONNECTION));
    if !have_profile {
        nmcli(&[
            "con", "add", "type", "wifi", "ifname", "wlan0", "con-name", AP_CONNECTION,
            "autoconnect", "no", "ssid", ssid,
            "802-11-wireless.mode", "ap", "802-11-wireless.band", "bg",
            "ipv4.method", "shared", "ipv6.method", "disabled",
        ])
        .await?;
    } else {
        // Keep the broadcast SSID in sync with the speaker name.
        nmcli(&["con", "modify", AP_CONNECTION, "802-11-wireless.ssid", ssid]).await?;
    }
    nmcli(&["con", "up", AP_CONNECTION]).await?;
    tracing::info!(%ssid, "onboarding AP started");
    Ok(())
}

pub async fn stop_ap() -> anyhow::Result<()> {
    nmcli(&["con", "down", AP_CONNECTION]).await?;
    tracing::info!("onboarding AP stopped");
    Ok(())
}

/// Split one line of `nmcli -t` output honoring `\:` escapes.
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut escape = false;
    for c in line.chars() {
        match (escape, c) {
            (true, c) => {
                cur.push(c);
                escape = false;
            }
            (false, '\\') => escape = true,
            (false, ':') => fields.push(std::mem::take(&mut cur)),
            (false, c) => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_split_honors_escapes() {
        assert_eq!(
            split_terse(r"6C\:3A\:FF\:58\:84\:4C:bt:disconnected"),
            vec!["6C:3A:FF:58:84:4C", "bt", "disconnected"]
        );
        assert_eq!(split_terse("a:b:"), vec!["a", "b", ""]);
        assert_eq!(split_terse(""), vec![""]);
    }
}
