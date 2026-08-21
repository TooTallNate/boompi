//! Wi-Fi management via NetworkManager's `nmcli`.
//!
//! nmcli's terse mode (`-t`) is a stable scripting interface and saves us
//! a few hundred lines of NM D-Bus proxies. Requires the boompid user to
//! be allowed by polkit (root on the appliance; `netdev` group on the dev
//! box).
//!
//! AP mode: a `shared` connection named [`AP_CONNECTION`] turns wlan0 into
//! an open access point with NM's built-in DHCP - the onboarding path when
//! no Wi-Fi is configured (see docs/PLAN.md Phase 5), and toggleable at any
//! time from the panel/web settings so the web UI stays reachable with no
//! shared network at all (camping mode).

#![cfg(target_os = "linux")]

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;

/// NM connection id (profile name) used for the onboarding access point.
pub const AP_CONNECTION: &str = "boompi-ap";

/// Last real scan results. A single radio can't scan while it hosts the
/// onboarding hotspot, so the captive-portal Wi-Fi step serves this cache
/// (refreshed just before the AP goes up) instead of an empty list.
static SCAN_CACHE: std::sync::Mutex<Vec<WifiNetwork>> = std::sync::Mutex::new(Vec::new());

const USER_DISCONNECTED_MARKER: &str = "/run/boompi/wifi-user-disconnected";
static USER_DISCONNECTED: AtomicBool = AtomicBool::new(false);

fn user_disconnected() -> bool {
    USER_DISCONNECTED.load(Ordering::Acquire)
        || std::path::Path::new(USER_DISCONNECTED_MARKER).exists()
}

fn set_user_disconnected(disconnected: bool) {
    USER_DISCONNECTED.store(disconnected, Ordering::Release);
    if disconnected {
        if let Some(parent) = std::path::Path::new(USER_DISCONNECTED_MARKER).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(USER_DISCONNECTED_MARKER, b"");
    } else {
        let _ = std::fs::remove_file(USER_DISCONNECTED_MARKER);
    }
}

#[derive(Debug, Default, Serialize)]
pub struct WifiStatus {
    /// A Wi-Fi capable device exists.
    pub supported: bool,
    /// Radio on?
    pub enabled: bool,
    /// SSID of the active connection, if any.
    pub connected: Option<String>,
    /// wlan IP when connected or in AP mode.
    pub ip: Option<String>,
    /// True while the boompi-ap profile is active (onboarding hotspot).
    pub ap_active: bool,
    /// Scan results, deduped by SSID (strongest kept), sorted by signal.
    pub networks: Vec<WifiNetwork>,
    /// Saved Wi-Fi profile names (connectable without a password).
    pub saved: Vec<String>,
}

// Shared with the protocol: scan results also travel as
// `ServerMessage::WifiNetworks` (BLE clients have no REST).
pub use boompi_proto::WifiNetwork;

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
            // Bare address: nmcli reports CIDR ("192.168.1.42/24"),
            // but the prefix length is noise to everyone reading a
            // settings page.
            st.ip = out
                .lines()
                .next()
                .and_then(|l| split_terse(l).get(1).cloned())
                .filter(|s| !s.is_empty())
                .and_then(|s| s.split('/').next().map(str::to_string));
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

        // NetworkManager may rejoin a saved profile as a side effect of
        // the scan, despite an earlier `dev disconnect`. Reassert the
        // explicit disconnect after scanning; ignore "not active" when
        // it remained down as requested.
        if user_disconnected() {
            let _ = nmcli(&["dev", "disconnect", &dev]).await;
            st.connected = None;
            st.ip = None;
            for net in &mut st.networks {
                net.in_use = false;
            }
        }
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
    // Joining from the onboarding hotspot: the radio cannot scan while
    // beaconing, so `dev wifi connect` fails with "no network with
    // SSID" on the first attempt (the retry only ever worked because
    // the AP-restore dance let a scan slip in between). Tear the AP
    // down first and wait for the target to appear in a scan.
    let ap_up = nmcli(&["-t", "-f", "NAME", "con", "show", "--active"])
        .await
        .map(|out| out.lines().any(|l| l == AP_CONNECTION))
        .unwrap_or(false);
    if ap_up {
        let _ = stop_ap().await;
        for attempt in 0..8 {
            let _ = nmcli(&["dev", "wifi", "rescan"]).await;
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            let visible = nmcli(&["-t", "-f", "SSID", "dev", "wifi", "list", "--rescan", "no"])
                .await
                .map(|out| out.lines().any(|l| l == ssid))
                .unwrap_or(false);
            if visible {
                break;
            }
            tracing::debug!(%ssid, attempt, "target SSID not in scan results yet");
        }
    }
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
        if let Err(err) = nmcli(&args).await {
            // A failed fresh join leaves a broken profile behind that NM
            // will keep retrying (and that shadows the next attempt);
            // drop it so a retry starts clean.
            if !saved {
                let _ = nmcli(&["con", "delete", "id", ssid]).await;
            }
            return Err(err);
        }
    }
    set_user_disconnected(false);
    tracing::info!(%ssid, "wifi connected");
    Ok(())
}

/// Cheap always-known facts for the protocol's [`WifiState`] broadcast:
/// no scan, no `settings_url` (the caller fills that in - it needs the
/// bound UI port).
pub async fn state() -> anyhow::Result<boompi_proto::WifiState> {
    let st = status(false).await?;
    let ap_ssid = if st.ap_active {
        nmcli(&[
            "-t",
            "-g",
            "802-11-wireless.ssid",
            "con",
            "show",
            AP_CONNECTION,
        ])
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    } else {
        None
    };
    Ok(boompi_proto::WifiState {
        supported: st.supported,
        enabled: st.enabled,
        connected: st.connected,
        ip: st.ip,
        ap_active: st.ap_active,
        ap_ssid,
        saved: st.saved,
        settings_url: None,
    })
}

/// Drop the current Wi-Fi connection without deleting its profile.
/// `nmcli dev disconnect` (not `con down`) on purpose: it also blocks
/// autoconnect until something is activated manually, so "leave my
/// home network" sticks instead of NM instantly rejoining.
pub async fn disconnect() -> anyhow::Result<()> {
    let devices = nmcli(&["-t", "-f", "DEVICE,TYPE", "dev"]).await?;
    let dev = devices
        .lines()
        .map(split_terse)
        .find(|f| f.len() >= 2 && f[1] == "wifi")
        .map(|f| f[0].clone())
        .ok_or_else(|| anyhow::anyhow!("no wifi device"))?;
    nmcli(&["dev", "disconnect", &dev]).await?;
    set_user_disconnected(true);
    tracing::info!(%dev, "wifi disconnected");
    Ok(())
}

/// Delete a saved profile (disconnects if active).
pub async fn forget(name: &str) -> anyhow::Result<()> {
    nmcli(&["con", "delete", "id", name]).await?;
    set_user_disconnected(false);
    tracing::info!(%name, "wifi profile forgotten");
    Ok(())
}

pub async fn set_radio(enabled: bool) -> anyhow::Result<()> {
    nmcli(&["radio", "wifi", if enabled { "on" } else { "off" }]).await?;
    set_user_disconnected(false);
    tracing::info!(enabled, "wifi radio toggled");
    Ok(())
}

/// Bring up the onboarding access point (open network, NM shared IPv4 -
/// NM runs its own DHCP). Creates the profile on first use.
pub async fn start_ap(ssid: &str) -> anyhow::Result<()> {
    set_user_disconnected(false);
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
            "con",
            "add",
            "type",
            "wifi",
            "ifname",
            "wlan0",
            "con-name",
            AP_CONNECTION,
            "autoconnect",
            "no",
            "ssid",
            ssid,
            "802-11-wireless.mode",
            "ap",
            "802-11-wireless.band",
            "bg",
            "ipv4.method",
            "shared",
            "ipv6.method",
            "disabled",
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
    set_user_disconnected(false);
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
