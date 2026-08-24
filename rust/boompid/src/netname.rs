//! Network-visible speaker name + DNS-SD adverts.
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
//! It also owns the box's own discovery advert: `_boompi._tcp` on the
//! protocol port, so control clients (the iOS app, web remote, other
//! boompis) can find boxes on the LAN without knowing an IP. The
//! instance name is the speaker name (full UTF-8 - no SMB filtering),
//! and TXT records carry the connection contract:
//!
//! | key       | value                                             |
//! |-----------|---------------------------------------------------|
//! | `txtvers` | TXT layout version, currently `1`                 |
//! | `id`      | stable box id (`boompi-XXXX`, matches hostname)   |
//! | `proto`   | WebSocket JSON protocol version ([`boompi_proto::PROTO_VERSION`]) |
//! | `ver`     | OS image version (`/etc/boompi-version`)         |
//! | `path`    | WebSocket path on the advertised port (`/ws`)     |
//!
//! avahi-daemon watches /etc/avahi/services with inotify and reloads
//! changed files on its own - no restarts, renames go live in
//! seconds. The image ships %h-wildcard baselines so discovery works
//! before boompid's first write.

#![cfg(target_os = "linux")]

use crate::state::SharedApp;

const SMB_SERVICE_PATH: &str = "/etc/avahi/services/smb.service";
const BOOMPI_SERVICE_PATH: &str = "/etc/avahi/services/boompi.service";

pub fn spawn(app: SharedApp, protocol_port: u16) {
    tokio::spawn(async move {
        let mut cfg = app.subscribe_cfg();
        let mut last: Option<String> = None;
        loop {
            let name = app.speaker_name().await;
            if last.as_deref() != Some(name.as_str()) {
                match write_smb_service(&name) {
                    Ok(()) => tracing::info!(%name, "SMB advert instance name updated"),
                    Err(err) => tracing::warn!(%err, "failed to write SMB avahi service"),
                }
                match write_boompi_service(&name, protocol_port) {
                    Ok(()) => tracing::info!(%name, "boompi advert instance name updated"),
                    Err(err) => tracing::warn!(%err, "failed to write boompi avahi service"),
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
/// Workaround: advertise a BMP-only instance name. Astral characters
/// are swapped for BMP stand-ins where a decent one exists (🔊 → ♪,
/// 💙 → ♥, 🇺🇸 → US) and dropped otherwise; emoji plumbing (variation
/// selector-16, ZWJ, skin tones) is dropped too. The AirPlay/
/// Bluetooth names keep the full emoji; only the SMB advert is
/// filtered.
fn smb_safe_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            // Emoji plumbing: invisible without their astral partner.
            '\u{FE0F}' | '\u{200D}' => {}
            c if (c as u32) <= 0xFFFF => out.push(c),
            c => {
                if let Some(sub) = bmp_substitute(c) {
                    out.push(sub);
                }
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Best-effort BMP stand-in for an astral (non-BMP) character, for
/// UIs that can't take the real thing (see [`smb_safe_name`]). The
/// aim is preserving the *flavor* of the name, not fidelity: every
/// speaker-ish emoji becomes a music note. `None` = drop it.
fn bmp_substitute(c: char) -> Option<char> {
    Some(match c {
        // Sound and music: speakers, notes, instruments, radio.
        '\u{1F508}'..='\u{1F50A}' /* 🔈🔉🔊 */
        | '\u{1F4E2}' | '\u{1F4E3}' /* 📢📣 */
        | '\u{1F3B5}' /* 🎵 */
        | '\u{1F3BC}' /* 🎼 */
        | '\u{1F3A4}' | '\u{1F3A7}' /* 🎤🎧 */
        | '\u{1F3B8}'..='\u{1F3BB}' /* 🎸🎹🎺🎻 */
        | '\u{1F941}' /* 🥁 */
        | '\u{1F4FB}' /* 📻 */ => '♪',
        '\u{1F3B6}' /* 🎶 */ => '♫',
        // Hearts of every color -> the one BMP heart.
        '\u{1F493}'..='\u{1F49F}' | '\u{1F5A4}' | '\u{1F9E1}'
        | '\u{1F90D}' | '\u{1F90E}' | '\u{1FA75}'..='\u{1FA77}' => '♥',
        // Celestial.
        '\u{1F31F}' | '\u{1F320}' /* 🌟🌠 */ => '★',
        '\u{1F31E}' /* 🌞 */ => '☀',
        '\u{1F311}'..='\u{1F31D}' /* moons */ => '☽',
        // Faces: happy-ish -> ☺, sad-ish -> ☹, rest dropped.
        '\u{1F600}'..='\u{1F60D}' | '\u{1F642}' | '\u{1F929}' => '☺',
        '\u{1F61E}'..='\u{1F62B}' | '\u{1F641}' => '☹',
        // Assorted one-to-ones.
        '\u{1F480}' /* 💀 */ => '☠',
        '\u{1F3E0}' /* 🏠 */ => '⌂',
        '\u{1F4A7}' /* 💧 */ => '☂',
        '\u{1F327}' /* 🌧 */ => '☔',
        '\u{1F3B2}' /* 🎲 */ => '⚁',
        '\u{1F0CF}' /* 🃏 */ => '♠',
        '\u{1F396}' /* 🎖 */ => '★',
        '\u{1F51D}' /* 🔝 */ => '↑',
        // Flags: regional indicator letters -> plain letters (🇺🇸 -> US).
        '\u{1F1E6}'..='\u{1F1FF}' => {
            char::from_u32('A' as u32 + (c as u32 - 0x1F1E6)).unwrap()
        }
        // Everything else (skin tones land here too): drop.
        _ => return None,
    })
}

fn write_smb_service(name: &str) -> std::io::Result<()> {
    let safe = smb_safe_name(name);
    let (instance, wildcards) = instance_name(&safe);
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
    write_atomic(SMB_SERVICE_PATH, &xml)
}

fn write_boompi_service(name: &str, port: u16) -> std::io::Result<()> {
    let xml = boompi_service_xml(
        name,
        port,
        crate::state::device_id(),
        crate::state::os_version(),
    );
    write_atomic(BOOMPI_SERVICE_PATH, &xml)
}

/// The `_boompi._tcp` control-protocol advert. Unlike SMB, DNS-SD TXT
/// consumers handle full UTF-8 instance names fine (same as AirPlay),
/// so the speaker name goes out unfiltered.
fn boompi_service_xml(name: &str, port: u16, id: &str, ver: &str) -> String {
    let (instance, wildcards) = instance_name(name.trim());
    let proto = boompi_proto::PROTO_VERSION;
    let id = xml_escape(id);
    let ver = xml_escape(ver);
    format!(
        r#"<?xml version="1.0" standalone='no'?><!--*-nxml-*-->
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<!-- Boompi control-protocol discovery: WebSocket + JSON API on the
     advertised port (see netname.rs for the TXT contract). Managed by
     boompid: manual edits are overwritten. -->
<service-group>
  <name replace-wildcards="{wildcards}">{instance}</name>
  <service>
    <type>_boompi._tcp</type>
    <port>{port}</port>
    <txt-record>txtvers=1</txt-record>
    <txt-record>id={id}</txt-record>
    <txt-record>proto={proto}</txt-record>
    <txt-record>ver={ver}</txt-record>
    <txt-record>path=/ws</txt-record>
  </service>
</service-group>
"#
    )
}

/// Instance-name XML for an avahi service file: the (escaped) name
/// itself, or the `%h` hostname wildcard when there is none.
fn instance_name(name: &str) -> (String, &'static str) {
    if name.is_empty() {
        ("%h".to_string(), "yes")
    } else {
        (xml_escape(name), "no")
    }
}

/// Atomic replace: avahi reloads on inotify, never sees a torn file.
fn write_atomic(path: &str, xml: &str) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, xml)?;
    std::fs::rename(&tmp, path)
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
    use super::{boompi_service_xml, smb_safe_name};

    #[test]
    fn boompi_advert_carries_name_port_and_txt() {
        let xml = boompi_service_xml("George's 🔊", 3001, "boompi-57fe", "v2.0.0");
        // Full UTF-8 instance name (emoji intact), XML-escaped.
        assert!(xml.contains(r#"<name replace-wildcards="no">George&apos;s 🔊</name>"#));
        assert!(xml.contains("<type>_boompi._tcp</type>"));
        assert!(xml.contains("<port>3001</port>"));
        assert!(xml.contains("<txt-record>id=boompi-57fe</txt-record>"));
        assert!(xml.contains(&format!(
            "<txt-record>proto={}</txt-record>",
            boompi_proto::PROTO_VERSION
        )));
        assert!(xml.contains("<txt-record>ver=v2.0.0</txt-record>"));
        assert!(xml.contains("<txt-record>path=/ws</txt-record>"));
    }

    #[test]
    fn boompi_advert_falls_back_to_hostname_wildcard() {
        let xml = boompi_service_xml("  ", 3001, "boompi-57fe", "dev");
        assert!(xml.contains(r#"<name replace-wildcards="yes">%h</name>"#));
    }

    #[test]
    fn boompi_advert_escapes_xml() {
        let xml = boompi_service_xml("A & B <Box>", 3001, "boompi-57fe", "dev");
        assert!(xml.contains(r#"<name replace-wildcards="no">A &amp; B &lt;Box&gt;</name>"#));
    }

    #[test]
    fn substitutes_astral_keeps_bmp() {
        assert_eq!(smb_safe_name("George’s 🔊"), "George’s ♪");
        assert_eq!(smb_safe_name("Nate's ♪ Box"), "Nate's ♪ Box");
        assert_eq!(smb_safe_name("🔊"), "♪");
        assert_eq!(smb_safe_name("Party 🎶 Box 💙"), "Party ♫ Box ♥");
        assert_eq!(smb_safe_name("USA 🇺🇸"), "USA US");
        assert_eq!(smb_safe_name("Happy 😀 / Sad 😢"), "Happy ☺ / Sad ☹");
        // Unmapped astral chars drop; VS16/ZWJ/skin tones too.
        assert_eq!(smb_safe_name("Fire 🔥 Box"), "Fire Box");
        assert_eq!(smb_safe_name("Wave 👋🏽"), "Wave");
        assert_eq!(smb_safe_name("A \u{FE0F}\u{200D} B"), "A B");
    }
}
