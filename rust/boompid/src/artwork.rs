//! AVRCP Cover Art (BIP over OBEX) fetcher.
//!
//! Implements the exact flow validated in Phase 0 (Spike C), with the rules
//! learned there baked in:
//!
//! - The obexd client session **dies with its creating D-Bus connection**,
//!   so this worker holds one long-lived connection and one session per
//!   device (reference: BlueZ `tools/mpris-proxy.c`, 5.81+).
//! - Phones (iOS at least) allow **one** BIP channel — competing clients
//!   get `Connection refused (no resources)`. `mpris-proxy` must not run
//!   alongside boompid.
//! - Sessions can go stale (phone reconnect/idle); a failed fetch drops the
//!   cached session and retries once with a fresh one.
//!
//! obexd normally lives on the session bus (dev setup); the appliance image
//! runs `obexd --system-bus`, so we fall back to the system bus.

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
        let mut counter = 0u64;
        while let Some(req) = rx.recv().await {
            let Some(handle) = req.handle.clone() else {
                // Prime the session so the phone starts including ImgHandle
                // in track metadata.
                if let Err(err) = ensure_session(&conn, &mut sessions, &req).await {
                    tracing::warn!(%err, address = %req.address, "BIP session prime failed");
                }
                continue;
            };
            counter += 1;
            match fetch(&conn, &mut sessions, &req, counter).await {
                Ok(bytes) => {
                    tracing::info!(%handle, size = bytes.len(), "cover art fetched");
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
            Ok(_transfer) => {
                let result = wait_for_file(&file).await;
                let _ = tokio::fs::remove_file(&file).await;
                return result;
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

async fn ensure_session(
    conn: &zbus::Connection,
    sessions: &mut HashMap<String, OwnedObjectPath>,
    req: &ArtRequest,
) -> anyhow::Result<OwnedObjectPath> {
    if let Some(path) = sessions.get(&req.address) {
        return Ok(path.clone());
    }
    let client = ObexClient1Proxy::new(conn).await?;
    let mut args: HashMap<&str, Value<'_>> = HashMap::new();
    args.insert("Target", Value::from("bip-avrcp"));
    args.insert("PSM", Value::from(req.psm));
    let path = client.create_session(&req.address, args).await?;
    tracing::info!(address = %req.address, session = %path, "BIP obex session established");
    sessions.insert(req.address.clone(), path.clone());
    Ok(path)
}

/// Wait for obexd to finish writing the transfer target file. The Transfer1
/// object vanishes on completion (racy to poll), so watch the file instead:
/// done when it exists, is non-empty, and its size is stable across polls.
async fn wait_for_file(path: &str) -> anyhow::Result<Bytes> {
    let mut last_size = 0u64;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Ok(meta) = tokio::fs::metadata(path).await else {
            continue;
        };
        let size = meta.len();
        if size > 0 && size == last_size {
            return Ok(tokio::fs::read(path).await?.into());
        }
        last_size = size;
    }
    anyhow::bail!("artwork transfer timed out")
}

/// Cache the image, record the handle→id mapping, and notify clients.
async fn publish(app: &SharedApp, resolved: &ResolvedArt, handle: &str, bytes: Bytes) {
    let id = art_id(&bytes);
    resolved
        .lock()
        .unwrap()
        .insert(handle.to_string(), id.clone());
    app.insert_art(id.clone(), bytes.clone()).await;

    let track = {
        let mut s = app.shared.write().await;
        match s.track.as_mut() {
            Some(track) => {
                track.artwork_id = Some(id);
                Some(track.clone())
            }
            None => None,
        }
    };
    if let Some(track) = track {
        app.broadcast_frame(encode_artwork_frame(&bytes));
        app.broadcast(ServerMessage::Track(track));
    }
}

/// Content-derived artwork id (cache key / URL path segment).
fn art_id(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
