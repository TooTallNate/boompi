//! Home Assistant integration over MQTT discovery.
//!
//! One HA *device* per boombox (identified by hostname, named after
//! the speaker), carrying: an `update` entity wired into the OTA flow
//! (install button + live progress), playback/battery/diagnostic
//! sensors, transport buttons, a volume number, a pairing switch,
//! screensaver + update-channel selects, and the album art as an
//! image entity.
//!
//! Architecture: the publisher is just another subscriber on the same
//! broadcast channel the WebSocket clients use (pre-serialized
//! ServerMessage JSON), and inbound MQTT commands deserialize into the
//! same ClientMessage handler every other control surface uses. The
//! task reconnects with backoff and follows config changes (broker
//! settings edits restart the session via the config generation
//! watch).

use std::time::Duration;

use boompi_proto::{
    ClientMessage, PairingAction, ScreensaverKind, ServerMessage, UpdateAction, UpdateChannel,
};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};
use serde_json::json;

use crate::state::SharedApp;

/// Stable per-box identifier: "boompi-" + the last four hex digits of
/// the SoC serial - computed directly rather than read from the
/// hostname, whose file can lag the serial-derived rename on a fresh
/// A/B slot (a boot where that happened re-registered every HA entity
/// under a duplicate device). Falls back to the hostname, then a dev
/// constant.
fn device_id() -> String {
    let serial_id = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|ci| {
            ci.lines()
                .find(|l| l.starts_with("Serial"))
                .and_then(|l| l.split_whitespace().last())
                .filter(|s| s.len() >= 4)
                .map(|s| format!("boompi-{}", &s[s.len() - 4..]))
        });
    serial_id
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "boompi-dev".into())
}

struct Cfg {
    host: String,
    port: u16,
    username: String,
    password: String,
}

async fn read_cfg(app: &SharedApp) -> Option<Cfg> {
    let s = app.shared.read().await;
    let broker = s.settings.mqtt_broker.trim().to_string();
    if broker.is_empty() {
        return None;
    }
    let (host, port) = match broker.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => (h.to_string(), port),
            Err(_) => (broker.clone(), 1883),
        },
        None => (broker.clone(), 1883),
    };
    Some(Cfg {
        host,
        port,
        username: s.settings.mqtt_username.clone(),
        password: s.settings.mqtt_password.clone(),
    })
}

pub async fn run(app: SharedApp) {
    let mut cfg_watch = app.subscribe_cfg();
    loop {
        let Some(cfg) = read_cfg(&app).await else {
            // Disabled: sleep until settings change.
            let _ = cfg_watch.changed().await;
            continue;
        };
        match session(&app, &cfg, &mut cfg_watch).await {
            Ok(()) => {
                // Config changed; reconnect with the new settings
                // immediately.
                tracing::info!("mqtt: settings changed; reconnecting");
            }
            Err(err) => {
                tracing::warn!(%err, host = %cfg.host, "mqtt session ended; retrying in 30s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                    _ = cfg_watch.changed() => {}
                }
            }
        }
    }
}

/// One connected session. Returns Ok(()) when the config changed
/// (caller reconnects), Err on connection failure.
async fn session(
    app: &SharedApp,
    cfg: &Cfg,
    cfg_watch: &mut tokio::sync::watch::Receiver<u64>,
) -> anyhow::Result<()> {
    let id = device_id();
    let base = format!("boompi/{id}");
    let avail_topic = format!("{base}/availability");

    let mut opts = MqttOptions::new(format!("boompid-{id}"), &cfg.host, cfg.port);
    opts.set_keep_alive(Duration::from_secs(30));
    if !cfg.username.is_empty() {
        opts.set_credentials(&cfg.username, &cfg.password);
    }
    opts.set_last_will(LastWill::new(
        &avail_topic,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));

    let (client, mut eventloop) = AsyncClient::new(opts, 32);
    let mut rx = app.tx.subscribe();

    // Wait for the initial ConnAck before publishing anything.
    loop {
        match eventloop.poll().await? {
            Event::Incoming(Packet::ConnAck(_)) => break,
            _ => continue,
        }
    }
    tracing::info!(host = %cfg.host, id, "mqtt connected; publishing HA discovery");

    client
        .publish(&avail_topic, QoS::AtLeastOnce, true, "online")
        .await?;
    publish_discovery(app, &client, &id, &base, &avail_topic).await?;
    publish_full_state(app, &client, &base).await?;
    client
        .subscribe(format!("{base}/set/#"), QoS::AtLeastOnce)
        .await?;

    // Diagnostics (CPU temperature) have no event source; publish on
    // a fixed cadence so HA's history isn't just connect-time samples.
    let mut diag_interval = tokio::time::interval(Duration::from_secs(60));
    diag_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = diag_interval.tick() => {
                publish_diag(&client, &base).await;
            }
            event = eventloop.poll() => {
                match event? {
                    Event::Incoming(Packet::Publish(p)) => {
                        let payload = String::from_utf8_lossy(&p.payload).to_string();
                        handle_command(app, &p.topic, &base, &payload).await;
                    }
                    _ => {}
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok(crate::state::Outbound::Message(text)) => {
                        if let Ok(msg) = serde_json::from_str::<ServerMessage>(&text) {
                            publish_server_message(app, &client, &base, &msg).await;
                        }
                    }
                    Ok(_) => {} // binary visualizer frames: not for MQTT
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Dropped some deltas; resync with a full state.
                        let _ = publish_full_state(app, &client, &base).await;
                    }
                    Err(_) => anyhow::bail!("broadcast channel closed"),
                }
            }
            _ = cfg_watch.changed() => {
                let _ = client.publish(&avail_topic, QoS::AtLeastOnce, true, "offline").await;
                let _ = client.disconnect().await;
                return Ok(());
            }
        }
    }
}

/// HA MQTT discovery configs: one retained config per entity, all
/// under a shared device block.
async fn publish_discovery(
    app: &SharedApp,
    client: &AsyncClient,
    id: &str,
    base: &str,
    avail_topic: &str,
) -> anyhow::Result<()> {
    let name = app.speaker_name().await;
    let device = json!({
        "identifiers": [id],
        "name": name,
        "manufacturer": "Boompi",
        "model": "Boompi boombox",
        "sw_version": crate::state::os_version(),
    });
    let avail = json!([{ "topic": avail_topic }]);

    let entities: Vec<(&str, &str, serde_json::Value)> = vec![
        (
            "update",
            "os",
            json!({
                "name": "Boompi OS",
                "state_topic": format!("{base}/update"),
                "command_topic": format!("{base}/set/update"),
                "payload_install": "install",
                "device_class": "firmware",
            }),
        ),
        (
            "sensor",
            "playback",
            json!({
                "name": "Playback",
                "state_topic": format!("{base}/state"),
                "value_template": "{{ value_json.state }}",
                "json_attributes_topic": format!("{base}/state"),
                "icon": "mdi:music",
            }),
        ),
        (
            "number",
            "volume",
            json!({
                "name": "Volume",
                "state_topic": format!("{base}/volume"),
                "command_topic": format!("{base}/set/volume"),
                "min": 0, "max": 100, "step": 1,
                "unit_of_measurement": "%",
                "icon": "mdi:volume-high",
            }),
        ),
        (
            "button",
            "play",
            json!({
                "name": "Play",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "play",
                "icon": "mdi:play",
            }),
        ),
        (
            "button",
            "pause",
            json!({
                "name": "Pause",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "pause",
                "icon": "mdi:pause",
            }),
        ),
        (
            "button",
            "next",
            json!({
                "name": "Next track",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "next",
                "icon": "mdi:skip-next",
            }),
        ),
        (
            "button",
            "previous",
            json!({
                "name": "Previous track",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "previous",
                "icon": "mdi:skip-previous",
            }),
        ),
        (
            "switch",
            "pairing",
            json!({
                "name": "Bluetooth pairing",
                "state_topic": format!("{base}/pairing"),
                "command_topic": format!("{base}/set/pairing"),
                "payload_on": "ON", "payload_off": "OFF",
                "icon": "mdi:bluetooth-settings",
            }),
        ),
        (
            "sensor",
            "battery",
            json!({
                "name": "Battery",
                "state_topic": format!("{base}/battery"),
                "value_template": "{{ value_json.percentage }}",
                "device_class": "battery",
                "unit_of_measurement": "%",
                "state_class": "measurement",
            }),
        ),
        (
            "sensor",
            "battery_voltage",
            json!({
                "name": "Battery voltage",
                "state_topic": format!("{base}/battery"),
                "value_template": "{{ value_json.voltage }}",
                "device_class": "voltage",
                "unit_of_measurement": "V",
                "state_class": "measurement",
                "suggested_display_precision": 2,
                "entity_category": "diagnostic",
            }),
        ),
        (
            "sensor",
            "battery_current",
            json!({
                "name": "Battery current",
                "state_topic": format!("{base}/battery"),
                "value_template": "{{ value_json.current }}",
                "device_class": "current",
                "unit_of_measurement": "A",
                "state_class": "measurement",
                "suggested_display_precision": 2,
                "entity_category": "diagnostic",
            }),
        ),
        (
            "sensor",
            "battery_power",
            json!({
                "name": "Battery power",
                "state_topic": format!("{base}/battery"),
                "value_template": "{{ value_json.power }}",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "suggested_display_precision": 1,
                "entity_category": "diagnostic",
            }),
        ),
        (
            "sensor",
            "battery_state",
            json!({
                "name": "Battery state",
                "state_topic": format!("{base}/battery"),
                "value_template": concat!(
                    "{% if value_json.full %}full",
                    "{% elif value_json.charging %}charging",
                    "{% elif value_json.current > 0.08 %}discharging",
                    "{% else %}idle{% endif %}",
                ),
                "device_class": "enum",
                "options": ["full", "charging", "discharging", "idle"],
                "icon": "mdi:battery-heart-variant",
            }),
        ),
        (
            "sensor",
            "battery_time_remaining",
            json!({
                "name": "Battery time remaining",
                "state_topic": format!("{base}/battery"),
                // Key is omitted while charging/full/unlearned; a
                // literal None renders the sensor unknown.
                "value_template": "{{ value_json.time_remaining_min | default('None') }}",
                "device_class": "duration",
                "unit_of_measurement": "min",
                "icon": "mdi:battery-clock",
            }),
        ),
        (
            "binary_sensor",
            "battery_charging",
            json!({
                "name": "Battery charging",
                "state_topic": format!("{base}/battery"),
                "value_template": "{{ 'ON' if value_json.charging else 'OFF' }}",
                "device_class": "battery_charging",
                "entity_category": "diagnostic",
            }),
        ),
        (
            "select",
            "screensaver",
            json!({
                "name": "Screensaver",
                "state_topic": format!("{base}/settings"),
                "value_template": "{{ value_json.screensaver }}",
                "command_topic": format!("{base}/set/screensaver"),
                "options": ["off", "clock", "matrix", "art"],
                "entity_category": "config",
                "icon": "mdi:monitor-star",
            }),
        ),
        (
            "select",
            "update_channel",
            json!({
                "name": "Update channel",
                "state_topic": format!("{base}/settings"),
                "value_template": "{{ value_json.update_channel }}",
                "command_topic": format!("{base}/set/channel"),
                "options": ["stable", "edge"],
                "entity_category": "config",
                "icon": "mdi:source-branch",
            }),
        ),
        (
            "button",
            "preview_screensaver",
            json!({
                "name": "Preview screensaver",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "preview_screensaver",
                "entity_category": "config",
                "icon": "mdi:monitor-eye",
            }),
        ),
        (
            "button",
            "reboot",
            json!({
                "name": "Reboot",
                "command_topic": format!("{base}/set/cmd"),
                "payload_press": "reboot",
                "device_class": "restart",
                "entity_category": "config",
            }),
        ),
        (
            "image",
            "album_art",
            json!({
                "name": "Album art",
                "url_topic": format!("{base}/art_url"),
                "icon": "mdi:image-album",
            }),
        ),
        (
            "sensor",
            "cpu_temp",
            json!({
                "name": "CPU temperature",
                "state_topic": format!("{base}/diag"),
                "value_template": "{{ value_json.cpu_temp }}",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "entity_category": "diagnostic",
            }),
        ),
    ];

    for (component, object, mut cfg) in entities {
        let obj = cfg.as_object_mut().unwrap();
        obj.insert("unique_id".into(), json!(format!("{id}_{object}")));
        // Entity names are device-scoped ("Battery current", not
        // "George's Battery current"): HA composes the friendly name
        // and derives a clean entity_id. Without this, entities
        // registered later than their siblings picked up a doubled
        // device prefix (sensor.georges_georges_battery_current).
        obj.insert("has_entity_name".into(), json!(true));
        obj.insert("device".into(), device.clone());
        obj.insert("availability".into(), avail.clone());
        let topic = format!("homeassistant/{component}/{id}_{object}/config");
        client
            .publish(topic, QoS::AtLeastOnce, true, cfg.to_string())
            .await?;
    }
    Ok(())
}

/// Push everything from a fresh snapshot (connect + resync).
async fn publish_full_state(
    app: &SharedApp,
    client: &AsyncClient,
    base: &str,
) -> anyhow::Result<()> {
    let snap = app.snapshot().await;
    publish_server_message(
        app,
        client,
        base,
        &ServerMessage::Source(snap.source.clone()),
    )
    .await;
    if let Some(track) = snap.track.clone() {
        publish_server_message(app, client, base, &ServerMessage::Track(track)).await;
    }
    publish_server_message(
        app,
        client,
        base,
        &ServerMessage::Volume { level: snap.volume },
    )
    .await;
    if let Some(b) = snap.battery.clone() {
        publish_server_message(app, client, base, &ServerMessage::Battery(b)).await;
    }
    publish_server_message(
        app,
        client,
        base,
        &ServerMessage::Pairing(snap.pairing.clone()),
    )
    .await;
    publish_server_message(
        app,
        client,
        base,
        &ServerMessage::Settings(snap.settings.clone()),
    )
    .await;
    publish_server_message(
        app,
        client,
        base,
        &ServerMessage::Update(snap.updates.clone()),
    )
    .await;
    publish_diag(client, base).await;
    Ok(())
}

fn kind_str(kind: ScreensaverKind) -> &'static str {
    match kind {
        ScreensaverKind::Off => "off",
        ScreensaverKind::Clock => "clock",
        ScreensaverKind::Matrix => "matrix",
        ScreensaverKind::Art => "art",
    }
}

/// Map a ServerMessage broadcast onto the MQTT state topics.
async fn publish_server_message(
    app: &SharedApp,
    client: &AsyncClient,
    base: &str,
    msg: &ServerMessage,
) {
    let publish = |topic: String, payload: String| {
        let client = client.clone();
        async move {
            let _ = client.publish(topic, QoS::AtLeastOnce, true, payload).await;
        }
    };
    match msg {
        ServerMessage::Track(t) => {
            // Merged into /state below via a fresh snapshot (state
            // depends on both source and track).
            let _ = t;
            publish_playback_state(app, client, base).await;
        }
        ServerMessage::Source(_) => {
            publish_playback_state(app, client, base).await;
        }
        ServerMessage::Volume { level } => {
            publish(
                format!("{base}/volume"),
                format!("{}", (level * 100.0).round() as i64),
            )
            .await;
        }
        ServerMessage::Battery(b) => {
            publish(
                format!("{base}/battery"),
                json!({
                    "percentage": (b.percentage * 100.0).round() as i64,
                    "voltage": (b.voltage * 100.0).round() / 100.0,
                    "current": (b.current * 100.0).round() / 100.0,
                    "power": (b.power * 100.0).round() / 100.0,
                    "charging": b.charging,
                    "full": b.full,
                    "low": b.low,
                    "time_remaining_min": b.time_remaining_secs.map(|s| s / 60),
                })
                .to_string(),
            )
            .await;
        }
        ServerMessage::Pairing(p) => {
            let on = matches!(
                p.state,
                boompi_proto::PairingState::Discoverable | boompi_proto::PairingState::Confirm
            );
            publish(
                format!("{base}/pairing"),
                if on { "ON" } else { "OFF" }.into(),
            )
            .await;
        }
        ServerMessage::Settings(s) => {
            publish(
                format!("{base}/settings"),
                json!({
                    "screensaver": kind_str(s.screensaver),
                    "update_channel": match s.update_channel {
                        UpdateChannel::Stable => "stable",
                        UpdateChannel::Edge => "edge",
                    },
                })
                .to_string(),
            )
            .await;
        }
        ServerMessage::Update(u) => {
            let latest = u.available.clone().unwrap_or_else(|| u.version.clone());
            let release_url = if latest.contains('-') {
                "https://github.com/TooTallNate/boompi/releases/tag/edge".to_string()
            } else {
                format!("https://github.com/TooTallNate/boompi/releases/tag/{latest}")
            };
            // HA compares versions with semver, where "-sha" means
            // PRERELEASE - an edge build would rank OLDER than its
            // base release and HA would show "Up-to-date" despite the
            // differing versions (bench). Render suffixed stamps in a
            // non-semver shape so HA falls back to plain string
            // comparison; clean stable tags keep real semver ordering.
            let ha_version = |v: &str| match v.split_once('-') {
                Some((b, sha)) => format!("{b} ({sha})"),
                None => v.to_string(),
            };
            publish(
                format!("{base}/update"),
                json!({
                    "installed_version": ha_version(&u.version),
                    "latest_version": ha_version(&latest),
                    "in_progress": u.applying.is_some(),
                    "update_percentage": u.progress.map(|p| (p * 100.0).round() as i64),
                    "release_url": release_url,
                    "title": "Boompi OS",
                })
                .to_string(),
            )
            .await;
        }
        _ => {}
    }
}

/// Playback sensor state + attributes (+ art URL when present).
async fn publish_playback_state(app: &SharedApp, client: &AsyncClient, base: &str) {
    let snap = app.snapshot().await;
    let state = match snap.track.as_ref().map(|t| t.status) {
        Some(boompi_proto::PlaybackStatus::Playing) => "playing",
        Some(boompi_proto::PlaybackStatus::Paused) => "paused",
        _ if snap.source.active.is_some() => "idle",
        _ => "off",
    };
    let track = snap.track.as_ref();
    let payload = json!({
        "state": state,
        "source": snap.source.active.map(|k| format!("{k:?}").to_lowercase()),
        "device": snap.source.device_name,
        "title": track.and_then(|t| t.title.clone()),
        "artist": track.and_then(|t| t.artist.clone()),
        "album": track.and_then(|t| t.album.clone()),
    });
    let _ = client
        .publish(
            format!("{base}/state"),
            QoS::AtLeastOnce,
            true,
            payload.to_string(),
        )
        .await;
    if let Some(art_id) = track.and_then(|t| t.artwork_id.as_ref()) {
        if let Some(url) = art_url(app, art_id).await {
            let _ = client
                .publish(format!("{base}/art_url"), QoS::AtLeastOnce, true, url)
                .await;
        }
    }
}

/// Best-effort LAN URL for an artwork id (HA fetches it itself).
async fn art_url(app: &SharedApp, art_id: &str) -> Option<String> {
    let url = app.settings_url()?;
    Some(format!("{}/art/{art_id}", url.trim_end_matches('/')))
}

/// Diagnostics (CPU temperature; Linux only).
async fn publish_diag(client: &AsyncClient, base: &str) {
    let temp = crate::state::read_diag().cpu_temp_c;
    if let Some(t) = temp {
        let _ = client
            .publish(
                format!("{base}/diag"),
                QoS::AtLeastOnce,
                true,
                json!({ "cpu_temp": t }).to_string(),
            )
            .await;
    }
}

/// Inbound command topics -> the shared ClientMessage handler.
async fn handle_command(app: &SharedApp, topic: &str, base: &str, payload: &str) {
    let Some(suffix) = topic.strip_prefix(&format!("{base}/set/")) else {
        return;
    };
    tracing::info!(suffix, payload, "mqtt command");
    let msg = match (suffix, payload) {
        ("cmd", "play") => Some(ClientMessage::Play),
        ("cmd", "pause") => Some(ClientMessage::Pause),
        ("cmd", "next") => Some(ClientMessage::Next),
        ("cmd", "previous") => Some(ClientMessage::Previous),
        ("cmd", "preview_screensaver") => Some(ClientMessage::PreviewScreensaver),
        ("cmd", "reboot") => {
            #[cfg(target_os = "linux")]
            {
                let _ = tokio::process::Command::new("systemctl")
                    .arg("reboot")
                    .spawn();
            }
            None
        }
        ("update", "install") => Some(ClientMessage::Update {
            action: UpdateAction::Apply,
        }),
        ("volume", v) => v.parse::<f32>().ok().map(|pct| ClientMessage::SetVolume {
            level: (pct / 100.0).clamp(0.0, 1.0),
        }),
        ("pairing", "ON") => Some(ClientMessage::Pairing {
            action: PairingAction::Enable,
        }),
        ("pairing", "OFF") => Some(ClientMessage::Pairing {
            action: PairingAction::Cancel,
        }),
        ("screensaver", v) => {
            let kind = match v {
                "clock" => ScreensaverKind::Clock,
                "matrix" => ScreensaverKind::Matrix,
                "art" => ScreensaverKind::Art,
                _ => ScreensaverKind::Off,
            };
            Some(ClientMessage::SetSettings(boompi_proto::SettingsPatch {
                screensaver: Some(kind),
                ..Default::default()
            }))
        }
        ("channel", v) => Some(ClientMessage::SetSettings(boompi_proto::SettingsPatch {
            update_channel: Some(if v == "edge" {
                UpdateChannel::Edge
            } else {
                UpdateChannel::Stable
            }),
            ..Default::default()
        })),
        _ => None,
    };
    if let Some(msg) = msg {
        app.handle_client_message(msg).await;
    }
}
