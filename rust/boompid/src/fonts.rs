//! Emoji font manager: the image ships Noto Color Emoji (free, OFL);
//! additional fonts download into /data/fonts (survives OTA) and the
//! active choice is enforced through fontconfig - on this stack the
//! fontconfig layer decides emoji selection (fontique's fontconfig
//! backend builds its generic Emoji list from `font_sort`), so boompid
//! writes /data/fontconfig/emoji.conf hiding every emoji family except
//! the chosen one, and restarts the panel UI to re-init its font
//! collection.
//!
//! Licensing: only freely redistributable fonts ship with the image.
//! The Apple entry downloads the community apple-emoji-linux build
//! directly from its GitHub release onto the user's own device at the
//! user's request; it is never redistributed by us.

#![cfg(target_os = "linux")]

use serde::Serialize;

pub const FONTS_DIR: &str = "/data/fonts";
pub const CONF_DIR: &str = "/data/fontconfig";
pub const CONF_PATH: &str = "/data/fontconfig/emoji.conf";

pub struct FontDef {
    pub id: &'static str,
    pub label: &'static str,
    /// Family name inside the font file (fontconfig reject patterns).
    pub family: &'static str,
    /// None = ships with the image (never downloaded or removed).
    pub download: Option<Download>,
    pub license: &'static str,
}

pub struct Download {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub const CATALOG: &[FontDef] = &[
    FontDef {
        id: "noto",
        label: "Noto Color Emoji",
        family: "Noto Color Emoji",
        download: None,
        license: "OFL - ships with the speaker",
    },
    FontDef {
        id: "twemoji",
        label: "Twemoji",
        family: "Twemoji Mozilla",
        download: Some(Download {
            url: "https://github.com/mozilla/twemoji-colr/releases/download/v0.7.0/Twemoji.Mozilla.ttf",
            sha256: "6d90152ee0d29e82fe2a87793af5aa4b7ad13e6538360889e141e81ed299ee8e",
            size: 1_474_284,
        }),
        license: "CC-BY 4.0 (Twitter/Mozilla)",
    },
    FontDef {
        id: "openmoji",
        label: "OpenMoji",
        family: "OpenMoji",
        download: Some(Download {
            url: "https://github.com/hfg-gmuend/openmoji/raw/17.0.0/font/OpenMoji-color-glyf_colr_0/OpenMoji-color-glyf_colr_0.ttf",
            sha256: "8376fee074649ece235faba0d157d851f3d93632f0540ad1165feba1aba9ba37",
            size: 2_681_320,
        }),
        license: "CC BY-SA 4.0 (HfG Gmuend)",
    },
    FontDef {
        id: "blobmoji",
        label: "Blobmoji",
        family: "Blobmoji",
        download: Some(Download {
            url: "https://github.com/C1710/blobmoji/releases/download/v15.0/Blobmoji.ttf",
            sha256: "dcc3d6675036ba9b35ef574d2221532770b5bd9886cba23c645ef46947eb78f9",
            size: 12_877_044,
        }),
        license: "Apache 2.0 - the classic Android blob style",
    },
    FontDef {
        id: "noto-mono",
        label: "Noto Emoji (monochrome)",
        family: "Noto Emoji",
        download: Some(Download {
            url: "https://github.com/google/fonts/raw/be7a91f7db2db749ebfc36f598eb85501127e7db/ofl/notoemoji/NotoEmoji%5Bwght%5D.ttf",
            sha256: "de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551",
            size: 1_982_596,
        }),
        license: "OFL - outline glyphs, tinted like text",
    },
    FontDef {
        id: "apple",
        label: "Apple Color Emoji",
        family: "Apple Color Emoji",
        download: Some(Download {
            url: "https://github.com/samuelngs/apple-emoji-linux/releases/download/macos-26-20260722-484daf4e/AppleColorEmoji-Linux.ttf",
            sha256: "e37c7af6265ac4a0af6d57bc65e86109a776d9966e8343334557f63da482516f",
            size: 115_969_256,
        }),
        license: "Apple's artwork - community apple-emoji-linux build, downloaded onto this speaker for personal use",
    },
];

pub fn def(id: &str) -> Option<&'static FontDef> {
    CATALOG.iter().find(|f| f.id == id)
}

pub fn font_path(id: &str) -> std::path::PathBuf {
    std::path::Path::new(FONTS_DIR).join(format!("{id}.ttf"))
}

pub fn installed(id: &str) -> bool {
    match def(id) {
        Some(f) if f.download.is_none() => true,
        Some(_) => font_path(id).exists(),
        None => false,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FontStatus {
    pub id: String,
    pub label: String,
    pub license: String,
    pub installed: bool,
    pub active: bool,
    pub builtin: bool,
    pub size: u64,
}

pub fn list(active: &str) -> Vec<FontStatus> {
    CATALOG
        .iter()
        .map(|f| FontStatus {
            id: f.id.into(),
            label: f.label.into(),
            license: f.license.into(),
            installed: installed(f.id),
            active: f.id == active,
            builtin: f.download.is_none(),
            size: f.download.as_ref().map(|d| d.size).unwrap_or(0),
        })
        .collect()
}

/// Write the fontconfig fragment enforcing the choice: every catalog
/// family except the chosen one is rejected, and the `emoji` alias
/// points at the winner. The image's /etc/fonts/local.conf includes
/// this file (ignore_missing) and scans /data/fonts.
pub fn write_conf(active: &str) -> anyhow::Result<()> {
    let chosen = def(active).ok_or_else(|| anyhow::anyhow!("unknown font id: {active}"))?;
    std::fs::create_dir_all(CONF_DIR)?;
    let mut rejects = String::new();
    for f in CATALOG.iter().filter(|f| f.id != active) {
        rejects.push_str(&format!(
            "    <pattern><patelt name=\"family\"><string>{}</string></patelt></pattern>\n",
            f.family
        ));
    }
    let conf = format!(
        r#"<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<!-- Generated by boompid (emoji font selection) - do not edit. -->
<fontconfig>
  <selectfont><rejectfont>
{rejects}  </rejectfont></selectfont>
  <alias binding="strong">
    <family>emoji</family>
    <prefer><family>{}</family></prefer>
  </alias>
</fontconfig>
"#,
        chosen.family
    );
    std::fs::write(CONF_PATH, conf)?;
    Ok(())
}

/// Rebuild fontconfig caches and bounce the panel so its font
/// collection re-initializes with the new visibility rules.
pub async fn apply_live() {
    let _ = tokio::process::Command::new("fc-cache")
        .arg("-f")
        .status()
        .await;
    let _ = tokio::process::Command::new("systemctl")
        .args(["restart", "boompi-ui"])
        .status()
        .await;
}

/// Boot-time reconciliation: if the persisted choice's font vanished
/// (fresh /data, manual deletion), fall back to the built-in.
pub fn reconcile(active: &str) -> &'static str {
    let id = if installed(active) { active } else { "noto" };
    let id = def(id).map(|d| d.id).unwrap_or("noto");
    if let Err(err) = write_conf(id) {
        tracing::warn!(%err, "emoji fontconfig write failed");
    }
    id
}

/// Download + verify a catalog font into /data/fonts (streamed - the
/// Apple build is ~110MB and the box has 1GB of RAM).
pub async fn download(id: &str) -> anyhow::Result<()> {
    use sha2::Digest;
    let f = def(id).ok_or_else(|| anyhow::anyhow!("unknown font id: {id}"))?;
    let dl = f
        .download
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("{id} ships with the image"))?;
    std::fs::create_dir_all(FONTS_DIR)?;
    let tmp = std::path::Path::new(FONTS_DIR).join(format!(".partial-{id}"));
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = sha2::Sha256::new();
    let mut resp = reqwest::get(dl.url).await?.error_for_status()?;
    let mut written: u64 = 0;
    while let Some(chunk) = resp.chunk().await? {
        use tokio::io::AsyncWriteExt;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
    }
    {
        use tokio::io::AsyncWriteExt;
        file.flush().await?;
    }
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    if digest != dl.sha256 || written != dl.size {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!("download verification failed ({written} bytes, sha256 {digest})");
    }
    std::fs::rename(&tmp, font_path(id))?;
    tracing::info!(id, bytes = written, "emoji font downloaded");
    Ok(())
}

pub fn remove(id: &str) -> anyhow::Result<()> {
    let f = def(id).ok_or_else(|| anyhow::anyhow!("unknown font id: {id}"))?;
    if f.download.is_none() {
        anyhow::bail!("built-in font cannot be removed");
    }
    std::fs::remove_file(font_path(id))?;
    Ok(())
}
