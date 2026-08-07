//! AVRCP Cover Art (BIP over OBEX) fetcher.
//!
//! Implements the exact flow validated in Phase 0 (Spike C), with the rules
//! learned there baked in:
//!
//! - The obexd client session **dies with its creating D-Bus connection**,
//!   so this worker holds one long-lived connection and one session per
//!   device (reference: BlueZ `tools/mpris-proxy.c`, 5.81+).
//! - Phones (iOS at least) allow **one** BIP channel - competing clients
//!   get `Connection refused (no resources)`. `mpris-proxy` must not run
//!   alongside boompid.
//! - Sessions can go stale (phone reconnect/idle); a failed fetch drops the
//!   cached session and retries once with a fresh one.
//!
//! obexd is hardcoded to a session bus. On the dev Pi (RPi OS) that's the
//! real user session bus; the appliance has no session bus, so the image
//! runs a private one (`obex-bus.service`, unix:path=/run/obex-bus) with
//! obexd on it, and boompid.service points DBUS_SESSION_BUS_ADDRESS at it -
//! `Connection::session()` below then lands on the right bus in both
//! worlds. (The system-bus fallback is a last resort and normally unused.)

#![cfg(target_os = "linux")]

use crate::state::SharedApp;
use boompi_proto::{encode_artwork_frame, ServerMessage};
use bytes::Bytes;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use zbus::zvariant::{OwnedObjectPath, Value};

/// Map of AVRCP image handle → resolved `artwork_id`, shared with the
/// bluetooth source so `Track` messages can carry the id once known.
pub type ResolvedArt = Arc<Mutex<HashMap<String, String>>>;

#[derive(Debug, Clone)]
pub struct ArtRequest {
    /// Device address, colon form ("6C:3A:FF:58:84:4C").
    pub address: String,
    /// BIP OBEX L2CAP PSM (`MediaPlayer1.ObexPort`; iOS uses 4105).
    pub psm: u16,
    /// `Track.ImgHandle` (7-digit string). `None` just primes the OBEX
    /// session: phones only include `ImgHandle` in track metadata while a
    /// BIP connection is alive, so the session must be established eagerly
    /// on `ObexPort` discovery (mpris-proxy does the same).
    pub handle: Option<String>,
}

#[zbus::proxy(
    interface = "org.bluez.obex.Client1",
    default_service = "org.bluez.obex",
    default_path = "/org/bluez/obex"
)]
trait ObexClient1 {
    fn create_session(
        &self,
        destination: &str,
        args: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;
    fn remove_session(&self, session: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.bluez.obex.Session1",
    default_service = "org.bluez.obex"
)]
trait ObexSession1 {
    #[zbus(property)]
    fn destination(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.bluez.obex.Image1",
    default_service = "org.bluez.obex"
)]
trait ObexImage1 {
    fn get_thumbnail(
        &self,
        target_file: &str,
        handle: &str,
    ) -> zbus::Result<(OwnedObjectPath, HashMap<String, zbus::zvariant::OwnedValue>)>;
}

#[zbus::proxy(
    interface = "org.bluez.obex.Transfer1",
    default_service = "org.bluez.obex"
)]
trait ObexTransfer1 {
    fn cancel(&self) -> zbus::Result<()>;
    #[zbus(property)]
    fn status(&self) -> zbus::Result<String>;
}

pub fn spawn(app: SharedApp, resolved: ResolvedArt) -> mpsc::UnboundedSender<ArtRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ArtRequest>();
    tokio::spawn(async move {
        let conn = match connect_obex_bus().await {
            Some(conn) => conn,
            None => {
                tracing::warn!("no D-Bus connection for obexd; cover art disabled");
                return;
            }
        };
        let mut sessions: HashMap<String, OwnedObjectPath> = HashMap::new();
        // Most recent fetch per device, replayed as keepalive traffic:
        // iOS idle-drops the BIP channel after ~2 minutes of OBEX
        // silence, refuses cold reconnects, and eventually stops minting
        // ImgHandles - the resolved-art cache made repeat albums go
        // silent long enough to hit exactly that (bench-observed:
        // "Connection refused (111)" and no art until re-pairing).
        let mut last_fetch: HashMap<String, ArtRequest> = HashMap::new();
        let mut counter = 0u64;
        let mut keepalive = tokio::time::interval(Duration::from_secs(45));
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let req = tokio::select! {
                req = rx.recv() => match req {
                    Some(req) => req,
                    None => break,
                },
                _ = keepalive.tick() => {
                    for (address, req) in last_fetch.clone() {
                        if !sessions.contains_key(&address) {
                            continue;
                        }
                        counter += 1;
                        if let Err(err) = fetch(&conn, &mut sessions, &req, counter).await {
                            tracing::debug!(%err, %address, "BIP keepalive fetch failed");
                        }
                    }
                    continue;
                }
            };
            let Some(handle) = req.handle.clone() else {
                // Prime the session so the phone starts including ImgHandle
                // in track metadata.
                if let Err(err) = ensure_session(&conn, &mut sessions, &req).await {
                    tracing::warn!(%err, address = %req.address, "BIP session prime failed");
                }
                continue;
            };
            counter += 1;
            tracing::info!(target: "boompid::flow", %handle, address = %req.address, "BIP art fetch start");
            match fetch(&conn, &mut sessions, &req, counter).await {
                Ok(bytes) => {
                    tracing::info!(%handle, size = bytes.len(), "cover art fetched");
                    last_fetch.insert(req.address.clone(), req.clone());
                    publish(&app, &resolved, &handle, bytes).await;
                }
                Err(err) => {
                    tracing::warn!(%err, %handle, "cover art fetch failed")
                }
            }
        }
    });
    tx
}

async fn connect_obex_bus() -> Option<zbus::Connection> {
    match zbus::Connection::session().await {
        Ok(conn) => Some(conn),
        Err(_) => zbus::Connection::system().await.ok(),
    }
}

async fn fetch(
    conn: &zbus::Connection,
    sessions: &mut HashMap<String, OwnedObjectPath>,
    req: &ArtRequest,
    counter: u64,
) -> anyhow::Result<Bytes> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let session = ensure_session(conn, sessions, req).await?;
        let file = format!("/tmp/boompi-art-{}-{counter}.img", std::process::id());
        let image = ObexImage1Proxy::builder(conn)
            .path(session.clone())?
            .build()
            .await?;
        let handle = req.handle.as_deref().unwrap_or_default();
        match image.get_thumbnail(&file, handle).await {
            Ok((transfer, _props)) => {
                let result = wait_for_transfer(conn, &transfer, &file).await;
                let _ = tokio::fs::remove_file(&file).await;
                match result {
                    Err(err) => {
                        // Burn the whole session, not just the transfer:
                        // an abandoned/stalled transfer leaves the BIP
                        // channel skewed - subsequent requests get the
                        // previous request's image (bench-observed: the
                        // same art id served for every following track).
                        // A fresh session starts the exchange clean.
                        tracing::debug!(%err, attempt, "cover art transfer bad; resetting session");
                        drop_session(conn, sessions, &req.address).await;
                        if attempt <= 2 {
                            continue;
                        }
                        return Err(err);
                    }
                    ok => return ok,
                }
            }
            Err(err) if attempt == 1 => {
                // Stale session (phone reconnected / dropped the channel):
                // recreate once and retry.
                tracing::debug!(%err, "obex session stale; recreating");
                sessions.remove(&req.address);
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// Tear a session down on both ends (cache + obexd) so the next fetch
/// negotiates a fresh BIP connection.
async fn drop_session(
    conn: &zbus::Connection,
    sessions: &mut HashMap<String, OwnedObjectPath>,
    address: &str,
) {
    if let Some(path) = sessions.remove(address) {
        if let Ok(client) = ObexClient1Proxy::new(conn).await {
            let _ = client.remove_session(&path).await;
        }
    }
}

async fn ensure_session(
    conn: &zbus::Connection,
    sessions: &mut HashMap<String, OwnedObjectPath>,
    req: &ArtRequest,
) -> anyhow::Result<OwnedObjectPath> {
    // A cached session dies whenever the phone drops the link (disconnect,
    // reconnect, idle) - obexd removes the object. Verify it still answers
    // before trusting it, else the eager re-prime after reconnection would
    // silently no-op and the phone would never mint ImgHandles again.
    if let Some(path) = sessions.get(&req.address) {
        if session_alive(conn, path).await {
            return Ok(path.clone());
        }
        tracing::info!(address = %req.address, "cached BIP session is dead; reconnecting");
        sessions.remove(&req.address);
    }
    let client = ObexClient1Proxy::new(conn).await?;
    let mut last_err = None;
    for backoff_ms in [0u64, 3000] {
        if backoff_ms > 0 {
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }
        let mut args: HashMap<&str, Value<'_>> = HashMap::new();
        args.insert("Target", Value::from("bip-avrcp"));
        args.insert("PSM", Value::from(req.psm));
        match client.create_session(&req.address, args).await {
            Ok(path) => {
                tracing::info!(address = %req.address, session = %path, "BIP obex session established");
                sessions.insert(req.address.clone(), path.clone());
                return Ok(path);
            }
            // Phones refuse cold BIP connects in some states (locked,
            // just-idle-dropped); one spaced retry often lands.
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.expect("at least one attempt").into())
}

async fn session_alive(conn: &zbus::Connection, path: &OwnedObjectPath) -> bool {
    match ObexSession1Proxy::builder(conn).path(path.clone()) {
        Ok(builder) => match builder.build().await {
            Ok(proxy) => proxy.destination().await.is_ok(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Wait for the OBEX transfer to actually finish, then read the file.
///
/// The old approach - "file size stable across two 200ms polls" - called
/// any transfer that stalled for 200ms complete, and truncated JPEGs are
/// content-addressed garbage once cached. Track the Transfer1 object's
/// Status instead: complete/error are authoritative; the object
/// vanishing (obexd frees it right after completion, or on cancel) is
/// disambiguated by whether the file decodes.
async fn wait_for_transfer(
    conn: &zbus::Connection,
    transfer: &OwnedObjectPath,
    path: &str,
) -> anyhow::Result<Bytes> {
    let proxy = ObexTransfer1Proxy::builder(conn)
        .path(transfer.clone())?
        .build()
        .await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match proxy.status().await {
            Ok(status) => match status.as_str() {
                "complete" => break,
                "error" => anyhow::bail!("obex transfer failed"),
                _ => {} // queued | active | suspended
            },
            // Object gone: completed-and-freed or cancelled - the file
            // content decides below.
            Err(_) => break,
        }
        if tokio::time::Instant::now() >= deadline {
            // Never abandon a live transfer silently: it keeps occupying
            // the OBEX channel and skews every response after it.
            let _ = proxy.cancel().await;
            anyhow::bail!("artwork transfer timed out");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let bytes: Bytes = tokio::fs::read(path).await?.into();
    if bytes.is_empty() || image::load_from_memory(&bytes).is_err() {
        anyhow::bail!(
            "transfer delivered undecodable image ({} bytes)",
            bytes.len()
        );
    }
    Ok(bytes)
}

/// Cache the image, record the handle→id mapping, and notify clients.
async fn publish(app: &SharedApp, resolved: &ResolvedArt, handle: &str, bytes: Bytes) {
    let id = publish_current_art(app, bytes, boompi_proto::SourceKind::Bluetooth).await;
    if !id.is_empty() {
        resolved.lock().unwrap().insert(handle.to_string(), id);
    }
}

/// Publish image bytes as the current track's artwork: cache it, stamp the
/// current track's `artwork_id`, and push both to clients. Shared by the
/// Bluetooth BIP fetcher and other sources (Spotify cover URLs, AirPlay).
///
/// `origin` is the source the art belongs to: fetches complete
/// asynchronously, so by the time bytes arrive another source may own the
/// display - stamping regardless would overwrite *its* artwork (this is
/// exactly how late BIP thumbnails were clobbering AirPlay covers). The
/// bytes are still cached so an existing `artwork_id` reference resolves.
pub async fn publish_current_art(
    app: &SharedApp,
    bytes: Bytes,
    origin: boompi_proto::SourceKind,
) -> String {
    // Central integrity gate for every art source: trim garbage tails and
    // reject anything that doesn't decode. OBEX (BIP) transfers can come
    // back truncated, and broken bytes are content-addressed - once cached
    // and referenced, the UI re-fetches the same garbage on every track
    // update.
    let trimmed = trim_image(&bytes);
    let bytes = if trimmed.len() == bytes.len() {
        bytes
    } else {
        Bytes::copy_from_slice(trimmed)
    };
    if bytes.is_empty() || image::load_from_memory(&bytes).is_err() {
        tracing::warn!(?origin, size = bytes.len(), "rejecting undecodable artwork");
        return String::new();
    }
    let id = art_id(&bytes);
    app.insert_art(id.clone(), bytes.clone()).await;

    let track = {
        let mut s = app.shared.write().await;
        if s.source.active != Some(origin) {
            tracing::warn!(
                target: "boompid::flow",
                ?origin,
                active = ?s.source.active,
                %id,
                size = bytes.len(),
                "art publish rejected: source no longer owns the display"
            );
            return id;
        }
        match s.track.as_mut() {
            Some(track) => {
                track.artwork_id = Some(id.clone());
                Some(track.clone())
            }
            None => None,
        }
    };
    tracing::info!(target: "boompid::flow", ?origin, %id, size = bytes.len(), stamped = track.is_some(), "art published");
    if let Some(track) = track {
        app.broadcast_frame(encode_artwork_frame(&bytes));
        app.broadcast(ServerMessage::Track(track));
    }
    id
}

/// Cut an image out of a buffer with a possible garbage tail (AirPlay
/// cover cache files are raw buffer dumps; OBEX transfers can trail).
/// Returns an empty slice when no plausible image is present.
pub fn trim_image(b: &[u8]) -> &[u8] {
    const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
    const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
    if b.starts_with(&JPEG_SOI) {
        // Trim to the last EOI marker. If tail garbage happens to contain
        // FF D9 we trim long, which decoders tolerate (they stop at the
        // real EOI).
        if let Some(eoi) = b.windows(2).rposition(|w| w == [0xFF, 0xD9]) {
            return &b[..eoi + 2];
        }
        return &[];
    }
    if b.starts_with(&PNG_MAGIC) {
        // Trim to the end of the IEND chunk (type + 4-byte CRC).
        if let Some(iend) = b.windows(4).rposition(|w| w == *b"IEND") {
            return b.get(..iend + 8).unwrap_or(&[]);
        }
        return &[];
    }
    b // unknown format: the decode gate still applies
}

/// Content-derived artwork id (cache key / URL path segment).
fn art_id(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
