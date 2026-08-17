//! Network-visible speaker name.
//!
//! The DNS hostname stays machine-safe (boompi-XXXX: ssh targets,
//! DHCP, avahi's mDNS collision handling all want plain ASCII). The
//! name humans see in discovery UIs is the DNS-SD service *instance*
//! name, which RFC 6763 explicitly makes user-visible UTF-8 - that is
//! how the AirPlay picker already shows the emoji. This module gives
//! the SMB (games share) advert the same treatment: the avahi service
//! file's instance name follows the speaker name, so the Finder
//! sidebar shows "George's" instead of "boompi-57fe". (Not the emoji,
//! though - see [`smb_safe_name`] for the macOS SMB bug that forbids
//! non-BMP characters here.)
//!
//! avahi-daemon watches /etc/avahi/services with inotify and reloads
//! changed files on its own - no restarts, renames go live in
//! seconds. The image ships a %h-wildcard baseline so discovery works
//! before boompid's first write.

#![cfg(target_os = "linux")]

use crate::state::SharedApp;

const SERVICE_PATH: &str = "/etc/avahi/services/smb.service";

pub fn spawn(app: SharedApp) {
    tokio::spawn(async move {
        let mut cfg = app.subscribe_cfg();
        let mut last: Option<String> = None;
        loop {
            let name = app.speaker_name().await;
            if last.as_deref() != Some(name.as_str()) {
                match write_service(&name) {
                    Ok(()) => tracing::info!(%name, "SMB advert instance name updated"),
                    Err(err) => tracing::warn!(%err, "failed to write SMB avahi service"),
                }
                last = Some(name);
            }
            if cfg.changed().await.is_err() {
                break;
            }
        }
    });
}

/// The advertised SMB instance name, minus characters macOS chokes on.
///
/// Empirically (macOS 26, smbutil + Finder both): an SMB DNS-SD
/// instance name containing any character outside the Basic
/// Multilingual Plane resolves fine but *fails session setup* -
/// "server rejected the authentication" - while BMP names (spaces,
/// curly quotes, ♪) connect happily. Something in Apple's SMB client
/// mangles UTF-16 surrogate pairs in the name it derives the
/// connection from. Emoji live above U+FFFF, so "George's 🔊"
/// becomes an entry that can be seen but never opened.
///
/// Workaround: advertise a BMP-only instance name. Emoji plumbing
/// characters (variation selector-16, ZWJ) are dropped too - they're
/// invisible orphans once their emoji is gone. The AirPlay/Bluetooth
/// names keep the full emoji; only the SMB advert is filtered.
fn smb_safe_name(name: &str) -> String {
    let filtered: String = name
        .chars()
        .filter(|&c| (c as u32) <= 0xFFFF && c != '\u{FE0F}' && c != '\u{200D}')
        .collect();
    filtered.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn write_service(name: &str) -> std::io::Result<()> {
    let safe = smb_safe_name(name);
    let (instance, wildcards) = if safe.is_empty() {
        ("%h".to_string(), "yes")
    } else {
        (xml_escape(&safe), "no")
    };
    let xml = format!(
        r#"<?xml version="1.0" standalone='no'?><!--*-nxml-*-->
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<!-- Finder/Explorer sidebar discovery for the guest games share.
     Managed by boompid (netname.rs): the instance name follows the
     speaker name. Manual edits are overwritten. -->
<service-group>
  <name replace-wildcards="{wildcards}">{instance}</name>
  <service>
    <type>_smb._tcp</type>
    <port>445</port>
  </service>
</service-group>
"#
    );
    // Atomic replace: avahi reloads on inotify, never sees a torn file.
    let tmp = format!("{SERVICE_PATH}.tmp");
    std::fs::write(&tmp, xml)?;
    std::fs::rename(&tmp, SERVICE_PATH)
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::smb_safe_name;

    #[test]
    fn strips_astral_keeps_bmp() {
        assert_eq!(smb_safe_name("George’s 🔊"), "George’s");
        assert_eq!(smb_safe_name("Nate's ♪ Box"), "Nate's ♪ Box");
        assert_eq!(smb_safe_name("🔊"), "");
        // VS16 + ZWJ orphans are dropped, whitespace collapsed.
        assert_eq!(smb_safe_name("A \u{FE0F}\u{200D} B 😀"), "A B");
    }
}
