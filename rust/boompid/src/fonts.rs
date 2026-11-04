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
//! The vendor sets (Apple, Samsung, Huawei, WhatsApp, Facebook,
//! Fluent 3D) come from community builds (apple-emoji-linux,
//! MFFMEmoji) and JoyPixels from its own CDN; each downloads directly
//! from the pinned upstream URL onto the user's own device at the
//! user's request and is never redistributed by us.
//!
//! Format: the images' source-built Skia draws CBDT, COLRv0 and COLRv1
//! (bench builds without ~/.boompi/skia-binaries link the stock
//! prebuilt, which lacks COLRv1). Each family name must be unique - the
//! reject rules match on it, and many Android builds reuse "Noto Color
//! Emoji".

#![cfg(target_os = "linux")]

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
        family: "Twemoji MFFM 17.0.3",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/Twemoji%2017.0.3%20COLRv1.ttf",
            sha256: "81a7cf19770b89702e43e5a204fda02b8e91935e323652a61d2c82acdf369332",
            size: 1_504_016,
        }),
        license: "CC-BY 4.0 (Twitter/jdecked) - community MFFMEmoji COLRv1 build",
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
    FontDef {
        id: "samsung",
        label: "Samsung One UI",
        family: "SamsungMFFMColorEmoji",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/OneUI%208.5.ttf",
            sha256: "650e1d7cfa34c6bb42283625b8da4cc4e87227dc9bc441bee66479062ad90d5c",
            size: 20_524_944,
        }),
        license: "Samsung's artwork - community MFFMEmoji build, downloaded onto this speaker for personal use",
    },
    FontDef {
        id: "fluent-3d",
        label: "Microsoft Fluent 3D",
        family: "FluentUI MFFM Emoji",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/Microsoft%20FluentUI%203D%2016.ttf",
            sha256: "afeae2411705772be83ed17ff71bf5de7cd11b5e44e0cfde0d8268a4ca648aac",
            size: 25_243_132,
        }),
        license: "MIT artwork (Microsoft) - community MFFMEmoji build",
    },
    FontDef {
        id: "huawei",
        label: "Huawei",
        family: "Huawei MFFM Emoji",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/Huawei%2016.0.ttf",
            sha256: "fe46f5a5622ade4588c24a5c6e8fe9b0a695a1dbbb2f30318a81f1db8ded6ac1",
            size: 22_592_504,
        }),
        license: "Huawei's artwork - community MFFMEmoji build, downloaded onto this speaker for personal use",
    },
    FontDef {
        id: "whatsapp",
        label: "WhatsApp",
        family: "WhatsApp MFFM Color Emoji",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/WhatsApp%202.26.8.72.ttf",
            sha256: "ba63a55c3a883ffa963c43cce2b9c38bb6e43f6dd3a716f47f1371cb78cfa39d",
            size: 35_388_128,
        }),
        license: "WhatsApp's artwork - community MFFMEmoji build, downloaded onto this speaker for personal use",
    },
    FontDef {
        id: "facebook",
        label: "Facebook",
        family: "Facebook MFFM Emoji",
        download: Some(Download {
            url: "https://raw.githubusercontent.com/mistu01/MFFMEmoji/2be82f0bc1d79559b2ecf3676827d12dcf02bda9/Files/Facebook%2016.0.ttf",
            sha256: "b1be92fc4ae3485337ad1cb2073010e3cbee1c8e6fe8a315728429bbd5c88db8",
            size: 39_425_672,
        }),
        license: "Meta's artwork - community MFFMEmoji build, downloaded onto this speaker for personal use",
    },
    FontDef {
        id: "joypixels",
        label: "JoyPixels",
        family: "JoyPixels",
        download: Some(Download {
            url: "https://cdn.joypixels.com/arch-linux/font/8.0.0/joypixels-android.ttf",
            sha256: "7c95040bebcb69676e81963ee0a82e4138f342ee85b5b8dbbad7d05d1f657eae",
            size: 13_578_288,
        }),
        license: "JoyPixels free license (personal use) - official 8.0 build from JoyPixels' CDN",
    },
    FontDef {
        id: "tossface",
        label: "Toss Face",
        family: "Toss Face Font Web",
        download: Some(Download {
            url: "https://github.com/toss/tossface/releases/download/v1.6.1/TossFaceFontWeb.otf",
            sha256: "e99b8642c24631f550aaa90644a17dc164367a79fc7c1584b1ea254b41db770b",
            size: 6_042_748,
        }),
        license: "Tossface license (Viva Republica) - official v1.6.1 release",
    },
];

pub fn def(id: &str) -> Option<&'static FontDef> {
    CATALOG.iter().find(|f| f.id == id)
}

pub fn font_path(id: &str) -> std::path::PathBuf {
    std::path::Path::new(FONTS_DIR).join(format!("{id}.ttf"))
}

/// A download counts only if it matches the current catalog pin: a file
/// left over from an older pin (same id, different build) would carry a
/// family name the reject rules no longer know about.
pub fn installed(id: &str) -> bool {
    match def(id).map(|f| f.download.as_ref()) {
        Some(None) => true,
        Some(Some(dl)) => std::fs::metadata(font_path(id)).is_ok_and(|m| m.len() == dl.size),
        None => false,
    }
}

/// Delete downloads that no longer match their catalog pin.
fn prune_stale() {
    for f in CATALOG.iter().filter(|f| f.download.is_some()) {
        let path = font_path(f.id);
        if path.exists() && !installed(f.id) {
            match std::fs::remove_file(&path) {
                Ok(()) => tracing::info!(id = f.id, "removed stale emoji font download"),
                Err(err) => tracing::warn!(%err, id = f.id, "stale emoji font removal failed"),
            }
        }
    }
}

pub fn list(active: &str) -> Vec<boompi_proto::EmojiFontInfo> {
    CATALOG
        .iter()
        .map(|f| boompi_proto::EmojiFontInfo {
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

/// Snapshot of the catalog + download state for clients (ws + REST).
pub async fn state(app: &crate::state::SharedApp) -> boompi_proto::EmojiFontsState {
    let s = app.shared.read().await;
    boompi_proto::EmojiFontsState {
        fonts: list(&s.emoji_font),
        downloading: s.emoji_download.clone(),
        progress: s.emoji_progress,
        error: s.emoji_error.clone(),
    }
}

/// Shared handler for the ws message and the REST endpoint.
pub async fn perform(
    app: &crate::state::SharedApp,
    action: boompi_proto::EmojiFontAction,
    id: &str,
) -> anyhow::Result<()> {
    use boompi_proto::EmojiFontAction as A;
    match action {
        A::Download => {
            {
                let mut s = app.shared.write().await;
                if s.emoji_download.is_some() {
                    anyhow::bail!("a download is already running");
                }
                s.emoji_download = Some(id.to_string());
                s.emoji_progress = Some(0.0);
                s.emoji_error = None;
            }
            broadcast_state(app).await;
            let app2 = app.clone();
            let id = id.to_string();
            tokio::spawn(async move {
                let result = download(&id, |done, total| {
                    let app3 = app2.clone();
                    async move {
                        let pct = done as f32 / total.max(1) as f32;
                        let significant = {
                            let mut s = app3.shared.write().await;
                            let prev = s.emoji_progress.unwrap_or(0.0);
                            let significant = pct - prev >= 0.01 || pct >= 1.0;
                            if significant {
                                s.emoji_progress = Some(pct);
                            }
                            significant
                        };
                        if significant {
                            broadcast_state(&app3).await;
                        }
                    }
                })
                .await;
                {
                    let mut s = app2.shared.write().await;
                    s.emoji_download = None;
                    s.emoji_progress = None;
                    if let Err(err) = &result {
                        tracing::warn!(%err, id, "emoji font download failed");
                        s.emoji_error = Some(err.to_string());
                    }
                }
                broadcast_state(&app2).await;
            });
        }
        A::Select => {
            if !installed(id) {
                anyhow::bail!("font not installed");
            }
            write_conf(id)?;
            app.shared.write().await.emoji_font = id.to_string();
            app.persist_config().await;
            broadcast_state(app).await;
            tokio::spawn(apply_live());
            tracing::info!(id, "emoji font selected");
        }
        A::Remove => {
            remove(id)?;
            let was_active = app.shared.read().await.emoji_font == id;
            if was_active {
                write_conf("noto")?;
                app.shared.write().await.emoji_font = "noto".into();
                app.persist_config().await;
                tokio::spawn(apply_live());
            }
            broadcast_state(app).await;
        }
    }
    Ok(())
}

async fn broadcast_state(app: &crate::state::SharedApp) {
    let snapshot = state(app).await;
    app.broadcast(boompi_proto::ServerMessage::EmojiFonts(snapshot));
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

/// Boot-time reconciliation: prune downloads from older catalog pins,
/// and if the persisted choice's font vanished (fresh /data, manual
/// deletion, stale pin), fall back to the built-in.
pub fn reconcile(active: &str) -> &'static str {
    prune_stale();
    let id = if installed(active) { active } else { "noto" };
    let id = def(id).map(|d| d.id).unwrap_or("noto");
    if let Err(err) = write_conf(id) {
        tracing::warn!(%err, "emoji fontconfig write failed");
    }
    id
}

/// Download + verify a catalog font into /data/fonts (streamed - the
/// Apple build is ~110MB and the box has 1GB of RAM). `progress` is
/// called with (bytes_done, bytes_total).
pub async fn download<F, Fut>(id: &str, progress: F) -> anyhow::Result<()>
where
    F: Fn(u64, u64) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
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
        progress(written, dl.size).await;
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
