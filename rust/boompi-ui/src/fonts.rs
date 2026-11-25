//! The system UI font (Settings.font) on the panel.
//!
//! Built-ins (Geist + Geist Pixel) are compiled in; uploaded and Google
//! Fonts families are served by boompid at `/fonts/ui/<file>` (see
//! boompid's uifonts.rs). Each installed family's files are fetched once
//! over HTTP, registered with fontique under the family name boompid
//! reports, and the window's `default-font-family` switches when the
//! selected family is renderable. Every Text without an explicit
//! family follows it; the Matrix rain keeps its own glyphs.

use crate::net::NetCtx;
use boompi_proto::{FontState, FontStatus};
use slint::{ModelRc, SharedString, VecModel};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Geist Pixel (vendored, OFL - see ui/fonts/OFL.txt). Registered here
/// rather than imported in app.slint: all five styles share the
/// typographic family "Geist Pixel" at the same weight, so imported
/// copies collapse into one family and only one style is reachable.
/// Overriding the family name gives each style its own family
/// (matching `boompi_proto::BUILTIN_FONTS`). Geist itself is imported
/// in app.slint.
pub fn register_builtin_fonts() {
    use slint::fontique_010::fontique;
    static FONTS: [(&str, &[u8]); 5] = [
        (
            "Geist Pixel Square",
            include_bytes!("../ui/fonts/GeistPixel-Square.ttf"),
        ),
        (
            "Geist Pixel Grid",
            include_bytes!("../ui/fonts/GeistPixel-Grid.ttf"),
        ),
        (
            "Geist Pixel Circle",
            include_bytes!("../ui/fonts/GeistPixel-Circle.ttf"),
        ),
        (
            "Geist Pixel Triangle",
            include_bytes!("../ui/fonts/GeistPixel-Triangle.ttf"),
        ),
        (
            "Geist Pixel Line",
            include_bytes!("../ui/fonts/GeistPixel-Line.ttf"),
        ),
    ];
    let mut collection = slint::fontique_010::shared_collection();
    for (family, data) in FONTS {
        let blob = fontique::Blob::new(Arc::new(data));
        let info = fontique::FontInfoOverride {
            family_name: Some(family),
            ..Default::default()
        };
        if collection.register_fonts(blob, Some(info)).is_empty() {
            eprintln!("built-in font {family:?} failed to register");
        }
    }
}

/// Register a downloaded font file under `family` at `weight`.
fn register(family: &str, weight: u16, data: Vec<u8>) -> bool {
    use slint::fontique_010::fontique;
    let mut collection = slint::fontique_010::shared_collection();
    let blob = fontique::Blob::new(Arc::new(data));
    let info = fontique::FontInfoOverride {
        family_name: Some(family),
        weight: Some(fontique::FontWeight::new(weight as f32)),
        ..Default::default()
    };
    !collection.register_fonts(blob, Some(info)).is_empty()
}

/// HTTP base of boompid from the WebSocket URL
/// ("ws://host:3001/ws" -> "http://host:3001").
pub fn http_base(ws_url: &str) -> String {
    let (scheme, rest) = match ws_url.split_once("://") {
        Some(("wss", rest)) => ("https", rest),
        Some((_, rest)) => ("http", rest),
        None => ("http", ws_url),
    };
    let host = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}://{host}")
}

/// What's been registered (or is being fetched), by URL, plus the
/// families that are renderable.
#[derive(Default)]
pub struct Registry {
    urls: HashSet<String>,
    families: HashSet<String>,
}

pub type SharedRegistry = Arc<Mutex<Registry>>;

/// Apply a FontState: fetch + register new files, then pick the family
/// to render (the selection once ready; Geist after a failed download;
/// unchanged while downloading).
pub fn apply_state(ctx: &NetCtx, state: FontState) {
    let base = ctx.http_base.clone();
    let registry = ctx.fonts.clone();
    let weak = ctx.weak.clone();
    let mut wanted = Vec::new();
    {
        let mut reg = registry.lock().unwrap();
        for fam in &state.installed {
            for file in &fam.files {
                if reg.urls.insert(file.url.clone()) {
                    wanted.push((fam.family.clone(), file.weight, file.url.clone()));
                }
            }
        }
    }
    tokio::spawn(async move {
        let mut fetched = Vec::new();
        for (family, weight, url) in wanted {
            let got = async {
                reqwest::get(format!("{base}{url}"))
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await
            }
            .await;
            match got {
                Ok(bytes) => fetched.push((family, weight, url, bytes.to_vec())),
                Err(err) => {
                    eprintln!("font fetch {url}: {err}");
                    registry.lock().unwrap().urls.remove(&url);
                }
            }
        }
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let mut reg = registry.lock().unwrap();
            for (family, weight, url, data) in fetched {
                if register(&family, weight, data) {
                    reg.families.insert(family);
                } else {
                    eprintln!("font {url} failed to register");
                    reg.urls.remove(&url);
                }
            }
            let renderable = |f: &str| boompi_proto::is_builtin_font(f) || reg.families.contains(f);
            let target = match state.status {
                FontStatus::Ready if renderable(&state.family) => Some(state.family.as_str()),
                FontStatus::Ready | FontStatus::Error => Some(boompi_proto::DEFAULT_FONT),
                FontStatus::Downloading => None,
            };
            if let Some(family) = target {
                ui.set_font_family(family.into());
            }
            let custom: Vec<SharedString> = state
                .installed
                .iter()
                .filter(|f| renderable(&f.family))
                .map(|f| f.family.as_str().into())
                .collect();
            ui.set_custom_fonts(ModelRc::new(VecModel::from(custom)));
            ui.set_font(state.family.as_str().into());
            ui.set_font_note(
                match state.status {
                    FontStatus::Ready => String::new(),
                    FontStatus::Downloading => format!("Downloading {}…", state.family),
                    FontStatus::Error => format!(
                        "Couldn't get {}: {}",
                        state.family,
                        state.error.as_deref().unwrap_or("download failed")
                    ),
                }
                .into(),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn http_base_from_ws() {
        assert_eq!(
            super::http_base("ws://127.0.0.1:3001/ws"),
            "http://127.0.0.1:3001"
        );
        assert_eq!(
            super::http_base("ws://boompi.local:3001"),
            "http://boompi.local:3001"
        );
    }
}
