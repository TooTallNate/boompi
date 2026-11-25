//! The system UI font (`Settings.font`): one family for the whole
//! panel and the web settings.
//!
//! Three kinds of family:
//! - built-ins ([`boompi_proto::BUILTIN_FONTS`]: Geist + five Geist
//!   Pixel styles), vendored in the panel and the web apps - nothing to
//!   do here;
//! - uploads: .ttf/.otf files the user posts from the web settings
//!   (`POST /api/fonts/upload`), validated with ttf-parser and named by
//!   the family inside the file;
//! - any Google Fonts family: the TTFs for the weights the panel uses
//!   download in the background once the setting changes (and again at
//!   boot if they vanished).
//!
//! Files live in [`fonts_dir`] (`/data/ui-fonts` on the box - a sibling
//! of the emoji fonts' /data/fonts, which fontconfig scans, so a UI font
//! never joins the system fallback chain) and are served over HTTP at
//! `/fonts/ui/<file>`; the panel and the box's web app load them from
//! there. Two small JSON manifests record what is installed:
//! `google.json` (the last downloaded Google family) and `uploads.json`.
//!
//! The Google Fonts catalog (search) comes from fonts.google.com's
//! public metadata - no API key, but no CORS either, so the search runs
//! here and rides the protocol (BLE-only clients included).

use crate::state::SharedApp;
use boompi_proto::{FontFamily, FontFile, FontSource, FontState, FontStatus, GoogleFont};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Upload size cap per file.
pub const UPLOAD_CAP: usize = 30 << 20;
/// Weights the panel draws with (light clock, body, semibold, bold).
const PANEL_WEIGHTS: [u16; 4] = [300, 400, 600, 700];
const CATALOG_URL: &str = "https://fonts.google.com/metadata/fonts";
const CSS_URL: &str = "https://fonts.googleapis.com/css2";
/// Refresh the cached catalog after this long (stale copies still serve
/// when the refresh fails - the box is often offline).
const CATALOG_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);
const SEARCH_LIMIT: usize = 40;

/// Where UI fonts live: `$BOOMPI_FONTS_DIR`, else `ui-fonts/` next to
/// the config (/data/boompi.toml on the box -> /data/ui-fonts, so it
/// survives OTA), else a temp dir.
pub fn fonts_dir(config_path: Option<&Path>) -> PathBuf {
    if let Some(dir) = std::env::var_os("BOOMPI_FONTS_DIR") {
        return PathBuf::from(dir);
    }
    match config_path.and_then(Path::parent) {
        Some(parent) => parent.join("ui-fonts"),
        None => std::env::temp_dir().join("boompi-ui-fonts"),
    }
}

// ---------------------------------------------------------------------------
// Manifests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredFile {
    weight: u16,
    file: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredFamily {
    family: String,
    files: Vec<StoredFile>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn google_manifest(dir: &Path) -> Option<StoredFamily> {
    read_json(&dir.join("google.json"))
}

fn uploads_manifest(dir: &Path) -> Vec<StoredFamily> {
    read_json(&dir.join("uploads.json")).unwrap_or_default()
}

/// A family counts as installed only while every file is still there.
fn complete(dir: &Path, fam: &StoredFamily) -> bool {
    !fam.files.is_empty() && fam.files.iter().all(|f| dir.join(&f.file).is_file())
}

fn to_proto(fam: &StoredFamily, source: FontSource) -> FontFamily {
    FontFamily {
        family: fam.family.clone(),
        source,
        files: fam
            .files
            .iter()
            .map(|f| FontFile {
                weight: f.weight,
                url: format!("/fonts/ui/{}", f.file),
            })
            .collect(),
    }
}

/// Installed families: uploads first (alphabetical), then the
/// downloaded Google family unless an upload shadows it.
pub fn installed(dir: &Path) -> Vec<FontFamily> {
    let mut uploads: Vec<StoredFamily> = uploads_manifest(dir)
        .into_iter()
        .filter(|f| complete(dir, f))
        .collect();
    uploads.sort_by_key(|f| f.family.to_lowercase());
    let mut out: Vec<FontFamily> = uploads
        .iter()
        .map(|f| to_proto(f, FontSource::Upload))
        .collect();
    if let Some(g) = google_manifest(dir) {
        if complete(dir, &g) && !out.iter().any(|f| f.family == g.family) {
            out.push(to_proto(&g, FontSource::Google));
        }
    }
    out
}

/// Can `family` be rendered right now without downloading anything?
fn available(dir: &Path, family: &str) -> bool {
    boompi_proto::is_builtin_font(family) || installed(dir).iter().any(|f| f.family == family)
}

/// File-name stem for a family: readable slug + a short hash so
/// families differing only in punctuation can't collide.
fn slug(family: &str) -> String {
    let mut out = String::new();
    for c in family.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let mut out: String = out.trim_end_matches('-').chars().take(40).collect();
    if out.is_empty() {
        out.push_str("font");
    }
    // FNV-1a: stable across builds (DefaultHasher isn't).
    let mut h: u32 = 0x811c_9dc5;
    for b in family.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    format!("{out}-{:06x}", h & 0xff_ffff)
}

/// Served file names: `g-`/`u-` prefix + slug + weight + extension.
pub fn valid_file_name(name: &str) -> bool {
    (name.starts_with("g-") || name.starts_with("u-"))
        && (name.ends_with(".ttf") || name.ends_with(".otf"))
        && name.len() < 128
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
        && !name.contains("..")
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Snapshot for clients (State.font and the Font broadcast).
pub async fn state(app: &SharedApp) -> FontState {
    let s = app.shared.read().await;
    FontState {
        family: s.settings.font.clone(),
        status: s.font_status,
        error: s.font_error.clone(),
        installed: installed(&app.fonts_dir),
    }
}

pub async fn broadcast_state(app: &SharedApp) {
    let snapshot = state(app).await;
    app.broadcast(boompi_proto::ServerMessage::Font(snapshot));
}

/// Make `Settings.font` renderable: nothing to do for built-ins and
/// installed families; otherwise download it from Google Fonts in the
/// background (Downloading -> Ready / Error). Called after the setting
/// changes and at boot. A newer call supersedes an in-flight download.
pub async fn activate(app: &SharedApp) {
    let (family, generation) = {
        let mut s = app.shared.write().await;
        s.font_generation += 1;
        (s.settings.font.clone(), s.font_generation)
    };
    let dir = app.fonts_dir.clone();
    if available(&dir, &family) {
        {
            let mut s = app.shared.write().await;
            s.font_status = FontStatus::Ready;
            s.font_error = None;
        }
        broadcast_state(app).await;
        return;
    }
    {
        let mut s = app.shared.write().await;
        s.font_status = FontStatus::Downloading;
        s.font_error = None;
    }
    broadcast_state(app).await;
    let app = app.clone();
    tokio::spawn(async move {
        let weights = match catalog(&app).await {
            Ok(cat) => cat
                .iter()
                .find(|f| f.family.eq_ignore_ascii_case(&family))
                .map(|f| f.weights.clone()),
            Err(err) => {
                tracing::debug!(%err, "font catalog unavailable; guessing weights");
                None
            }
        };
        let result = download_google(&dir, &family, weights.as_deref()).await;
        {
            let mut s = app.shared.write().await;
            if s.font_generation != generation {
                return; // superseded by a newer selection
            }
            match &result {
                Ok(n) => {
                    tracing::info!(%family, files = n, "google font downloaded");
                    s.font_status = FontStatus::Ready;
                    s.font_error = None;
                }
                Err(err) => {
                    tracing::warn!(%err, %family, "google font download failed");
                    s.font_status = FontStatus::Error;
                    s.font_error = Some(format!("{err:#}"));
                }
            }
        }
        broadcast_state(&app).await;
    });
}

/// Validate a requested family name (`None` = reject the patch field).
pub fn sanitize_family(raw: &str) -> Option<String> {
    let family = raw.trim();
    if family.is_empty()
        || family.len() > boompi_proto::FONT_NAME_MAX
        || family.chars().any(|c| c.is_control())
    {
        return None;
    }
    Some(family.to_string())
}

// ---------------------------------------------------------------------------
// Google Fonts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub family: String,
    pub category: String,
    pub weights: Vec<u16>,
    /// Rank (1 = most popular).
    pub popularity: u32,
}

/// Parse fonts.google.com/metadata/fonts (optionally `)]}'`-prefixed).
pub fn parse_catalog(raw: &str) -> anyhow::Result<Vec<CatalogEntry>> {
    let start = raw
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("catalog: no JSON"))?;
    let doc: serde_json::Value = serde_json::from_str(&raw[start..])?;
    let list = doc["familyMetadataList"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("catalog: no familyMetadataList"))?;
    let mut out = Vec::with_capacity(list.len());
    for f in list {
        let Some(family) = f["family"].as_str() else {
            continue;
        };
        let mut weights: Vec<u16> = f["fonts"]
            .as_object()
            .map(|m| m.keys().filter_map(|k| k.parse().ok()).collect())
            .unwrap_or_default();
        weights.sort_unstable();
        weights.dedup();
        if weights.is_empty() {
            continue; // italic-only (or malformed) entries
        }
        out.push(CatalogEntry {
            family: family.to_string(),
            category: f["category"].as_str().unwrap_or("").to_string(),
            weights,
            popularity: f["popularity"].as_u64().unwrap_or(u32::MAX as u64) as u32,
        });
    }
    if out.is_empty() {
        anyhow::bail!("catalog: empty");
    }
    Ok(out)
}

/// The Google Fonts catalog: memory, then the on-disk cache, then the
/// network (a stale cache still answers when the fetch fails).
pub async fn catalog(app: &SharedApp) -> anyhow::Result<Arc<Vec<CatalogEntry>>> {
    let mut slot = app.font_catalog.lock().await;
    if let Some(cat) = slot.as_ref() {
        return Ok(cat.clone());
    }
    let cache = app.fonts_dir.join("google-catalog.json");
    let cached: Option<Vec<CatalogEntry>> = read_json(&cache);
    let fresh = std::fs::metadata(&cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < CATALOG_MAX_AGE);
    if let (Some(cat), true) = (&cached, fresh) {
        let cat = Arc::new(cat.clone());
        *slot = Some(cat.clone());
        return Ok(cat);
    }
    let fetched = async {
        let body = http()
            .get(CATALOG_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_catalog(&body)
    }
    .await;
    let cat = match (fetched, cached) {
        (Ok(cat), _) => {
            let _ = std::fs::create_dir_all(&app.fonts_dir);
            if let Err(err) = write_json(&cache, &cat) {
                tracing::warn!(%err, "font catalog cache write failed");
            }
            tracing::info!(families = cat.len(), "google fonts catalog fetched");
            cat
        }
        (Err(err), Some(stale)) => {
            tracing::warn!(%err, "font catalog refresh failed; using the stale cache");
            stale
        }
        (Err(err), None) => return Err(err.context("couldn't load the Google Fonts catalog")),
    };
    let cat = Arc::new(cat);
    *slot = Some(cat.clone());
    Ok(cat)
}

/// Case-insensitive substring search, prefix matches first, then by
/// popularity. An empty query lists the most popular families.
pub fn search(cat: &[CatalogEntry], query: &str) -> Vec<GoogleFont> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<&CatalogEntry> = cat
        .iter()
        .filter(|f| q.is_empty() || f.family.to_lowercase().contains(&q))
        .collect();
    hits.sort_by_key(|f| (!f.family.to_lowercase().starts_with(&q), f.popularity));
    hits.into_iter()
        .take(SEARCH_LIMIT)
        .map(|f| GoogleFont {
            family: f.family.clone(),
            category: f.category.clone(),
            weights: f.weights.clone(),
        })
        .collect()
}

/// Answer a FontSearch (broadcast - clients match on `query`).
pub async fn handle_search(app: SharedApp, query: String) {
    let msg = match catalog(&app).await {
        Ok(cat) => boompi_proto::ServerMessage::FontResults {
            fonts: search(&cat, &query),
            query,
            error: None,
        },
        Err(err) => boompi_proto::ServerMessage::FontResults {
            query,
            fonts: Vec::new(),
            error: Some(format!("{err:#}")),
        },
    };
    app.broadcast(msg);
}

/// The family's closest weight to each panel weight (deduplicated).
pub fn pick_weights(available: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = PANEL_WEIGHTS
        .iter()
        .filter_map(|&want| {
            available
                .iter()
                .copied()
                .min_by_key(|&w| (w.abs_diff(want), w))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// css2 API URL for upright `weights` (no axis spec for a lone 400, so
/// static families without a wght axis still resolve).
pub fn css_url(family: &str, weights: &[u16]) -> String {
    let mut fam = String::new();
    for b in family.bytes() {
        match b {
            b' ' => fam.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => fam.push(b as char),
            _ => fam.push_str(&format!("%{b:02X}")),
        }
    }
    if weights == [400] || weights.is_empty() {
        format!("{CSS_URL}?family={fam}")
    } else {
        let list: Vec<String> = weights.iter().map(u16::to_string).collect();
        format!("{CSS_URL}?family={fam}:wght@{}", list.join(";"))
    }
}

/// (weight, url) pairs from a css2 response. Requested without a
/// User-Agent, Google answers with one plain TTF per weight.
pub fn parse_css(css: &str) -> Vec<(u16, String)> {
    let mut out = Vec::new();
    for block in css.split("@font-face").skip(1) {
        let block = block.split('}').next().unwrap_or("");
        if block.contains("font-style: italic") {
            continue;
        }
        let weight = block
            .split("font-weight:")
            .nth(1)
            .and_then(|r| r.trim().split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|w| w.parse::<u16>().ok());
        let url = block
            .split("url(")
            .nth(1)
            .and_then(|r| r.split(')').next())
            .map(|u| u.trim_matches(|c| c == '\'' || c == '"').to_string());
        if let (Some(w), Some(u)) = (weight, url) {
            if u.starts_with("https://fonts.gstatic.com/") && !out.iter().any(|(ow, _)| *ow == w) {
                out.push((w, u));
            }
        }
    }
    out
}

fn http() -> reqwest::Client {
    // No User-Agent on purpose: css2 then serves plain TTF URLs (with a
    // browser UA it hands out unicode-range-split WOFF2s).
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("reqwest client")
}

async fn fetch_css(family: &str, weights: &[u16]) -> anyhow::Result<String> {
    let resp = http().get(css_url(family, weights)).send().await?;
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        anyhow::bail!("Google Fonts has no family named {family:?}");
    }
    Ok(resp.error_for_status()?.text().await?)
}

/// Download `family`'s panel weights into `dir` and make it the
/// recorded Google family (older Google downloads are deleted).
/// Returns the number of files.
pub async fn download_google(
    dir: &Path,
    family: &str,
    catalog_weights: Option<&[u16]>,
) -> anyhow::Result<usize> {
    let css = match catalog_weights {
        Some(w) => fetch_css(family, &pick_weights(w)).await?,
        // Unknown weights (catalog offline): ask for the panel set,
        // falling back to the family's default style.
        None => match fetch_css(family, &PANEL_WEIGHTS).await {
            Ok(css) => css,
            Err(_) => fetch_css(family, &[400]).await?,
        },
    };
    let faces = parse_css(&css);
    if faces.is_empty() {
        anyhow::bail!("Google Fonts returned no usable files for {family:?}");
    }
    std::fs::create_dir_all(dir)?;
    let stem = slug(family);
    let mut files = Vec::new();
    for (weight, url) in faces {
        let bytes = http()
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let parsed = parse_font(&bytes)?;
        let file = format!("g-{stem}-{weight}.{}", parsed.ext);
        let tmp = dir.join(format!(".partial-{file}"));
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, dir.join(&file))?;
        files.push(StoredFile { weight, file });
    }
    let manifest = StoredFamily {
        family: family.to_string(),
        files,
    };
    write_json(&dir.join("google.json"), &manifest)?;
    // One Google family on disk at a time.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("g-") && !manifest.files.iter().any(|f| f.file == name) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(manifest.files.len())
}

// ---------------------------------------------------------------------------
// Uploads
// ---------------------------------------------------------------------------

/// What an uploaded file turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFont {
    pub family: String,
    pub weight: u16,
    /// "ttf" (TrueType outlines) or "otf" (CFF).
    pub ext: &'static str,
}

/// Typographic subfamily names that are just weights/styles (dropped);
/// anything else ("Square", "Condensed") is part of how users name it.
fn is_style_word(sub: &str) -> bool {
    const WORDS: &[&str] = &[
        "regular", "normal", "book", "roman", "italic", "oblique", "thin", "hairline", "extra",
        "ultra", "light", "medium", "semi", "demi", "bold", "heavy", "black",
    ];
    sub.split(|c: char| c == ' ' || c == '-')
        .filter(|w| !w.is_empty())
        .all(|w| {
            let mut w = w.to_lowercase();
            for word in WORDS {
                w = w.replace(word, "");
            }
            w.is_empty()
        })
}

/// Parse a font file and name it: the typographic family (name ID 16,
/// else 1), plus the typographic subfamily (17) when it's a design
/// variant rather than a weight - so Geist Pixel's five styles, which
/// share family "Geist Pixel", upload as "Geist Pixel Square" etc.
pub fn parse_font(data: &[u8]) -> anyhow::Result<ParsedFont> {
    let ext = match data.get(..4) {
        Some([0, 1, 0, 0]) | Some(b"true") => "ttf",
        Some(b"OTTO") => "otf",
        Some(b"wOF2") | Some(b"wOFF") => {
            anyhow::bail!("WOFF/WOFF2 web fonts aren't supported - upload the .ttf or .otf")
        }
        Some(b"ttcf") => anyhow::bail!("font collections (.ttc) aren't supported"),
        _ => anyhow::bail!("not a TrueType/OpenType font"),
    };
    let face =
        ttf_parser::Face::parse(data, 0).map_err(|e| anyhow::anyhow!("unreadable font: {e}"))?;
    let name = |id: u16| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let mut family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(ttf_parser::name_id::FAMILY))
        .ok_or_else(|| anyhow::anyhow!("font has no family name"))?;
    if let Some(sub) = name(ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY) {
        if !is_style_word(&sub) && !family.ends_with(&sub) {
            family = format!("{family} {sub}");
        }
    }
    let family = sanitize_family(&family)
        .ok_or_else(|| anyhow::anyhow!("font family name is unusable: {family:?}"))?;
    Ok(ParsedFont {
        family,
        weight: face.weight().to_number().clamp(1, 1000),
        ext,
    })
}

/// Validate + store one uploaded file. Re-uploading a family's weight
/// replaces it.
pub fn store_upload(dir: &Path, data: &[u8]) -> anyhow::Result<ParsedFont> {
    if data.len() > UPLOAD_CAP {
        anyhow::bail!("font exceeds the {}MB cap", UPLOAD_CAP >> 20);
    }
    let parsed = parse_font(data)?;
    if boompi_proto::is_builtin_font(&parsed.family) {
        anyhow::bail!("{} is already built in", parsed.family);
    }
    std::fs::create_dir_all(dir)?;
    let file = format!(
        "u-{}-{}.{}",
        slug(&parsed.family),
        parsed.weight,
        parsed.ext
    );
    let tmp = dir.join(format!(".partial-{file}"));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, dir.join(&file))?;
    let mut uploads = uploads_manifest(dir);
    match uploads.iter_mut().find(|f| f.family == parsed.family) {
        Some(fam) => {
            fam.files
                .retain(|f| f.weight != parsed.weight && f.file != file);
            fam.files.push(StoredFile {
                weight: parsed.weight,
                file,
            });
            fam.files.sort_by_key(|f| f.weight);
        }
        None => uploads.push(StoredFamily {
            family: parsed.family.clone(),
            files: vec![StoredFile {
                weight: parsed.weight,
                file,
            }],
        }),
    }
    write_json(&dir.join("uploads.json"), &uploads)?;
    Ok(parsed)
}

/// Delete an installed family (upload, or the downloaded Google one).
/// Returns false when nothing matched.
pub fn remove(dir: &Path, family: &str) -> anyhow::Result<bool> {
    let mut found = false;
    let mut uploads = uploads_manifest(dir);
    if let Some(i) = uploads.iter().position(|f| f.family == family) {
        for f in &uploads[i].files {
            let _ = std::fs::remove_file(dir.join(&f.file));
        }
        uploads.remove(i);
        write_json(&dir.join("uploads.json"), &uploads)?;
        found = true;
    }
    if let Some(g) = google_manifest(dir).filter(|g| g.family == family) {
        for f in &g.files {
            let _ = std::fs::remove_file(dir.join(&f.file));
        }
        std::fs::remove_file(dir.join("google.json"))?;
        found = true;
    }
    Ok(found)
}

/// Handle FontRemove: delete, revert the setting if it was selected.
pub async fn handle_remove(app: &SharedApp, family: &str) {
    match remove(&app.fonts_dir, family) {
        Ok(true) => tracing::info!(%family, "ui font removed"),
        Ok(false) => tracing::info!(%family, "ui font remove: not installed"),
        Err(err) => tracing::warn!(%err, %family, "ui font removal failed"),
    }
    let was_active = app.shared.read().await.settings.font == family;
    if was_active && !boompi_proto::is_builtin_font(family) {
        let settings = {
            let mut s = app.shared.write().await;
            s.settings.font = boompi_proto::DEFAULT_FONT.into();
            s.settings.clone()
        };
        app.broadcast(boompi_proto::ServerMessage::Settings(settings));
        app.persist_config().await;
        activate(app).await;
    } else {
        broadcast_state(app).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXEL_SQUARE: &[u8] = include_bytes!("../../boompi-ui/ui/fonts/GeistPixel-Square.ttf");
    const GEIST: &[u8] = include_bytes!("../../boompi-ui/ui/fonts/Geist.ttf");
    const MATRIX: &[u8] = include_bytes!("../../boompi-ui/ui/fonts/Matrix-Code.ttf");

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boompid-uifonts-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn family_names_from_files() {
        // Geist Pixel styles share the typographic family; the design
        // subfamily keeps them apart.
        let p = parse_font(PIXEL_SQUARE).unwrap();
        assert_eq!(p.family, "Geist Pixel Square");
        assert_eq!(p.weight, 400);
        assert_eq!(p.ext, "ttf");
        let g = parse_font(GEIST).unwrap();
        assert_eq!(g.family, "Geist");
        assert!(parse_font(b"not a font at all").is_err());
        assert!(parse_font(b"wOF2\0\0\0\0")
            .unwrap_err()
            .to_string()
            .contains("WOFF"));
        assert!(is_style_word("SemiBold Italic"));
        assert!(is_style_word("ExtraLight"));
        assert!(!is_style_word("Square"));
    }

    #[test]
    fn upload_store_replace_remove() {
        let dir = tmpdir("upload");
        // Built-in families can't be shadowed by an upload.
        let e = store_upload(&dir, PIXEL_SQUARE).unwrap_err();
        assert!(e.to_string().contains("built in"), "{e}");
        let p = store_upload(&dir, MATRIX).unwrap();
        let family = p.family.clone();
        assert!(!family.is_empty());
        // Re-upload replaces, doesn't duplicate.
        store_upload(&dir, MATRIX).unwrap();
        let fams = installed(&dir);
        assert_eq!(fams.len(), 1);
        assert_eq!(fams[0].source, FontSource::Upload);
        assert_eq!(fams[0].files.len(), 1);
        let file = fams[0].files[0].url.trim_start_matches("/fonts/ui/");
        assert!(valid_file_name(file), "{file}");
        assert!(dir.join(file).is_file());
        assert!(available(&dir, &family));
        assert!(store_upload(&dir, b"garbage").is_err());
        assert!(remove(&dir, &family).unwrap());
        assert!(installed(&dir).is_empty());
        assert!(!dir.join(file).exists());
        assert!(!remove(&dir, "Nope").unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_names_and_slugs() {
        assert!(valid_file_name("u-geist-pixel-square-0a1b2c-400.ttf"));
        assert!(!valid_file_name("../google.json"));
        assert!(!valid_file_name("google.json"));
        assert!(!valid_file_name("u-a/../b.ttf"));
        assert_ne!(slug("A B"), slug("A-B"));
        assert!(slug("Press Start 2P").starts_with("press-start-2p-"));
    }

    #[test]
    fn google_helpers() {
        assert_eq!(
            pick_weights(&[100, 200, 300, 400, 500, 600, 700, 800, 900]),
            [300, 400, 600, 700]
        );
        assert_eq!(pick_weights(&[400]), [400]);
        assert_eq!(pick_weights(&[400, 700]), [400, 700]);
        assert_eq!(
            css_url("Press Start 2P", &[400]),
            "https://fonts.googleapis.com/css2?family=Press+Start+2P"
        );
        assert_eq!(
            css_url("Inter", &[300, 700]),
            "https://fonts.googleapis.com/css2?family=Inter:wght@300;700"
        );
        let css = "@font-face {\n  font-family: 'Inter';\n  font-style: normal;\n  font-weight: 300;\n  src: url(https://fonts.gstatic.com/s/inter/a.ttf) format('truetype');\n}\n@font-face {\n  font-family: 'Inter';\n  font-style: normal;\n  font-weight: 700;\n  src: url(https://fonts.gstatic.com/s/inter/b.ttf) format('truetype');\n}\n";
        assert_eq!(
            parse_css(css),
            vec![
                (300, "https://fonts.gstatic.com/s/inter/a.ttf".to_string()),
                (700, "https://fonts.gstatic.com/s/inter/b.ttf".to_string())
            ]
        );
        let raw = r#")]}'
{"familyMetadataList":[
 {"family":"Inter","category":"Sans Serif","fonts":{"400":{},"700":{},"400i":{}},"popularity":3},
 {"family":"Roboto","category":"Sans Serif","fonts":{"400":{}},"popularity":1},
 {"family":"Interstate Italic","category":"Serif","fonts":{"400i":{}},"popularity":2}]}"#;
        let cat = parse_catalog(raw).unwrap();
        assert_eq!(cat.len(), 2);
        assert_eq!(cat[0].weights, [400, 700]);
        let hits = search(&cat, "");
        assert_eq!(hits[0].family, "Roboto");
        let hits = search(&cat, "inT");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].family, "Inter");
    }
}
