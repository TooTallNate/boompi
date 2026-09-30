use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

fn path(index: u8) -> OwnedObjectPath {
    format!("/org/bluez/hci{index}").try_into().unwrap()
}

fn objects(entries: &[(u8, bool, bool)]) -> ManagedObjects {
    entries
        .iter()
        .map(|&(index, powered, network)| {
            let mut interfaces = HashMap::from([(
                ADAPTER.try_into().unwrap(),
                HashMap::from([("Powered".into(), OwnedValue::from(powered))]),
            )]);
            if network {
                interfaces.insert(NETWORK.try_into().unwrap(), HashMap::new());
            }
            (path(index), interfaces)
        })
        .collect()
}

fn removed(sender: &str, index: u8, interface: &str) -> Message {
    Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
        .unwrap()
        .sender(sender)
        .unwrap()
        .build(&(path(index), vec![interface]))
        .unwrap()
}

fn power(sender: &str, index: u8, value: Option<bool>) -> Message {
    let (changed, invalidated) = match value {
        Some(value) => (
            HashMap::from([("Powered", OwnedValue::from(value))]),
            Vec::<&str>::new(),
        ),
        None => (HashMap::new(), vec!["Powered"]),
    };
    Message::signal(path(index), PROPERTIES, "PropertiesChanged")
        .unwrap()
        .sender(sender)
        .unwrap()
        .build(&(ADAPTER, changed, invalidated))
        .unwrap()
}

#[test]
fn selects_all_capable_adapters_without_assuming_hci0() {
    let mut objects = objects(&[(0, true, false), (3, false, true), (7, true, true)]);
    objects.insert(
        path(9),
        HashMap::from([(NETWORK.try_into().unwrap(), HashMap::new())]),
    );
    assert_eq!(
        adapters(&objects),
        HashMap::from([(path(3), false), (path(7), true)])
    );
}

#[test]
fn absent_or_invalid_powered_is_not_ready() {
    let mut objects = objects(&[(1, true, true), (2, true, true)]);
    objects
        .get_mut(&path(1))
        .unwrap()
        .get_mut(ADAPTER)
        .unwrap()
        .clear();
    objects
        .get_mut(&path(2))
        .unwrap()
        .get_mut(ADAPTER)
        .unwrap()
        .insert("Powered".into(), OwnedValue::from(1u32));
    assert!(adapters(&objects).values().all(|powered| !powered));
}

#[test]
fn removal_invalidates_only_that_adapter_and_detects_path_reuse() {
    let mut state = State::default();
    state.set_owner(Some(":1.42".into()));
    state.registered.extend([path(1), path(2)]);
    state.signal(&removed(":1.42", 1, NETWORK)).unwrap();
    assert_eq!(state.registered, HashSet::from([path(2)]));
    assert_eq!(state.generation(&path(1)), 1);
    assert_eq!(state.generation(&path(2)), 0);
    state.signal(&removed(":1.42", 2, ADAPTER)).unwrap();
    assert!(state.registered.is_empty());
}

#[test]
fn power_changes_and_invalidations_resync_without_dropping_ownership() {
    let mut state = State::default();
    state.set_owner(Some(":1.42".into()));
    state.registered.insert(path(7));
    let revision = state.revision;
    for value in [Some(false), Some(true), None] {
        state.signal(&power(":1.42", 7, value)).unwrap();
    }
    assert_eq!(state.revision, revision + 3);
    assert!(state.registered.contains(&path(7)));
    assert_eq!(state.generation(&path(7)), 0);
}

#[test]
fn ignores_other_senders_and_unrelated_interfaces() {
    let mut state = State::default();
    state.set_owner(Some(":1.42".into()));
    state.registered.insert(path(7));
    let revision = state.revision;
    state.signal(&removed(":1.41", 7, NETWORK)).unwrap();
    state
        .signal(&removed(":1.42", 7, "org.bluez.Device1"))
        .unwrap();
    state.signal(&power(":1.43", 7, Some(false))).unwrap();
    assert_eq!(state.revision, revision);
    assert!(state.registered.contains(&path(7)));
}

#[test]
fn owner_replacement_clears_registration_and_ignores_old_owner() {
    let mut state = State::default();
    state.set_owner(Some(":1.42".into()));
    state.registered.insert(path(7));
    let changed = Message::signal("/org/freedesktop/DBus", DBUS, "NameOwnerChanged")
        .unwrap()
        .sender(DBUS)
        .unwrap()
        .build(&(BLUEZ, ":1.42", ":1.43"))
        .unwrap();
    state.signal(&changed).unwrap();
    assert_eq!(state.owner.as_deref(), Some(":1.43"));
    assert!(state.registered.is_empty());
    let revision = state.revision;
    state.signal(&removed(":1.42", 7, NETWORK)).unwrap();
    assert_eq!(state.revision, revision);
}

// These tests never use the host's system/session bus or Bluetooth devices.
// DBUS_DAEMON can point at a non-PATH installation; absence is an explicit skip.
struct TestBus {
    child: Child,
    address: String,
}

impl TestBus {
    fn start() -> Option<Self> {
        let executable = std::env::var_os("DBUS_DAEMON").unwrap_or_else(|| "dbus-daemon".into());
        let child = Command::new(executable)
            .args([
                concat!(
                    "--config-file=",
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/bus.conf"
                ),
                "--nofork",
                "--print-address=1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn();
        let child = match child {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("SKIP private-bus test: dbus-daemon not found (set DBUS_DAEMON)");
                return None;
            }
            Err(error) => panic!("starting private dbus-daemon: {error}"),
        };
        let mut bus = Self {
            child,
            address: String::new(),
        };
        BufReader::new(bus.child.stdout.take().unwrap())
            .read_line(&mut bus.address)
            .unwrap();
        bus.address = bus.address.trim().to_owned();
        assert!(
            !bus.address.is_empty(),
            "private dbus-daemon failed to start"
        );
        Some(bus)
    }

    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(2))
            .build()
            .await
            .unwrap()
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct Model {
    adapters: Vec<(u8, bool, bool)>,
    attempts: HashMap<u8, usize>,
    registered: HashMap<u8, String>,
    fail: HashSet<u8>,
    snapshots: usize,
    fail_snapshot: bool,
    snapshot_gate: Option<Arc<tokio::sync::Notify>>,
    register_gate: Option<Arc<tokio::sync::Notify>>,
}

struct Manager(Arc<Mutex<Model>>);

#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Manager {
    async fn get_managed_objects(&self) -> zbus::fdo::Result<ManagedObjects> {
        let (objects, gate) = {
            let mut model = self.0.lock().unwrap();
            model.snapshots += 1;
            if model.fail_snapshot {
                return Err(zbus::fdo::Error::Failed("temporary snapshot error".into()));
            }
            (objects(&model.adapters), model.snapshot_gate.take())
        };
        if let Some(gate) = gate {
            gate.notified().await;
        }
        Ok(objects)
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum NetworkError {
    AlreadyExists(String),
    Failed(String),
}

struct Network {
    index: u8,
    model: Arc<Mutex<Model>>,
}

#[zbus::interface(name = "org.bluez.NetworkServer1")]
impl Network {
    async fn register(
        &self,
        uuid: &str,
        bridge: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> std::result::Result<(), NetworkError> {
        assert_eq!((uuid, bridge), ("nap", "br-pan"));
        let gate = {
            let mut model = self.model.lock().unwrap();
            *model.attempts.entry(self.index).or_default() += 1;
            if model.registered.contains_key(&self.index) {
                return Err(NetworkError::AlreadyExists("NAP already registered".into()));
            }
            if model.fail.contains(&self.index) {
                return Err(NetworkError::Failed("bridge/adapter not ready".into()));
            }
            model
                .registered
                .insert(self.index, header.sender().unwrap().to_string());
            model.register_gate.take()
        };
        if let Some(gate) = gate {
            gate.notified().await;
        }
        Ok(())
    }

    fn unregister(&self, _uuid: &str) {
        panic!("the registrar must never unregister by UUID");
    }
}

async fn fake_bluez(bus: &TestBus, model: &Arc<Mutex<Model>>) -> Connection {
    let connection = bus.connect().await;
    connection
        .object_server()
        .at("/", Manager(model.clone()))
        .await
        .unwrap();
    for index in 0..4 {
        connection
            .object_server()
            .at(
                path(index),
                Network {
                    index,
                    model: model.clone(),
                },
            )
            .await
            .unwrap();
    }
    connection.request_name(BLUEZ).await.unwrap();
    connection
}

async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("timed out waiting for registrar");
}

async fn announce(connection: &Connection, model: &Arc<Mutex<Model>>, index: u8) {
    let interfaces = objects(&model.lock().unwrap().adapters)
        .remove(&path(index))
        .unwrap();
    connection
        .emit_signal(
            None::<&str>,
            "/",
            OBJECT_MANAGER,
            "InterfacesAdded",
            &(path(index), interfaces),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn private_bus_late_bluez_usb_power_removal_and_restart() {
    let Some(bus) = TestBus::start() else {
        return;
    };
    let client = bus.connect().await;
    let task_connection = client.clone();
    // A long retry interval makes the subsequent assertions require signals.
    let task = tokio::spawn(async move {
        serve(&task_connection, Duration::from_secs(60))
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let model = Arc::new(Mutex::new(Model::default()));
    let bluez = fake_bluez(&bus, &model).await;
    until(|| model.lock().unwrap().snapshots > 0).await;
    model.lock().unwrap().adapters = vec![(0, true, false), (1, false, true), (2, true, true)];
    announce(&bluez, &model, 1).await;
    announce(&bluez, &model, 2).await;
    until(|| model.lock().unwrap().registered.contains_key(&2)).await;
    assert!(!model.lock().unwrap().attempts.contains_key(&0));
    assert!(!model.lock().unwrap().attempts.contains_key(&1));

    model.lock().unwrap().adapters[1].1 = true;
    bluez
        .send(&power(bluez.unique_name().unwrap().as_str(), 1, None))
        .await
        .unwrap();
    until(|| model.lock().unwrap().registered.contains_key(&1)).await;
    for powered in [false, true] {
        let snapshots = model.lock().unwrap().snapshots;
        model.lock().unwrap().adapters[1].1 = powered;
        bluez
            .send(&power(
                bluez.unique_name().unwrap().as_str(),
                1,
                Some(powered),
            ))
            .await
            .unwrap();
        until(|| model.lock().unwrap().snapshots > snapshots).await;
    }
    assert_eq!(model.lock().unwrap().attempts[&1], 1);

    // Send removal and readdition without yielding to the registrar. A final
    // snapshot alone would miss this lifetime change at the same object path.
    model.lock().unwrap().registered.remove(&2);
    bluez
        .send(&removed(bluez.unique_name().unwrap().as_str(), 2, NETWORK))
        .await
        .unwrap();
    announce(&bluez, &model, 2).await;
    until(|| model.lock().unwrap().attempts[&2] == 2).await;
    assert_eq!(model.lock().unwrap().attempts[&1], 1);

    bluez.close().await.unwrap();
    model.lock().unwrap().registered.clear();
    let replacement = fake_bluez(&bus, &model).await;
    until(|| model.lock().unwrap().registered.len() == 2).await;
    assert_eq!(model.lock().unwrap().attempts[&1], 2);
    assert_eq!(model.lock().unwrap().attempts[&2], 3);
    assert!(model
        .lock()
        .unwrap()
        .registered
        .values()
        .all(|owner| Some(owner.as_str()) == client.unique_name().map(|name| name.as_str())));

    // Check the bus observes our connection disappearing on graceful shutdown.
    let observer = DBusProxy::new(&replacement).await.unwrap();
    let client_name = client.unique_name().unwrap().to_owned();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    client.close().await.unwrap();
    assert!(!observer.name_has_owner(client_name.into()).await.unwrap());
    replacement.close().await.unwrap();
}

#[tokio::test]
async fn private_bus_retries_errors_and_conflicts_without_churning_successes() {
    let Some(bus) = TestBus::start() else {
        return;
    };
    let model = Arc::new(Mutex::new(Model {
        adapters: vec![(1, true, true), (2, true, true), (3, true, true)],
        registered: HashMap::from([(2, ":other-owner".into())]),
        fail: HashSet::from([3]),
        ..Default::default()
    }));
    let bluez = fake_bluez(&bus, &model).await;
    let client = bus.connect().await;
    let task_connection = client.clone();
    let task = tokio::spawn(async move {
        serve(&task_connection, Duration::from_millis(50))
            .await
            .unwrap();
    });
    until(|| model.lock().unwrap().attempts.get(&3).copied().unwrap_or(0) >= 3).await;
    assert_eq!(model.lock().unwrap().attempts[&1], 1);
    assert_eq!(model.lock().unwrap().registered[&2], ":other-owner");
    assert!(model.lock().unwrap().attempts[&2] >= 2);

    // Temporary enumeration failure must not forget successful registrations.
    model.lock().unwrap().fail_snapshot = true;
    let snapshots = model.lock().unwrap().snapshots;
    until(|| model.lock().unwrap().snapshots >= snapshots + 2).await;
    model.lock().unwrap().fail_snapshot = false;
    model.lock().unwrap().fail.clear();
    model.lock().unwrap().registered.remove(&2);
    until(|| model.lock().unwrap().registered.len() == 3).await;
    let attempts = model.lock().unwrap().attempts.clone();
    let snapshots = model.lock().unwrap().snapshots;
    until(|| model.lock().unwrap().snapshots >= snapshots + 2).await;
    assert_eq!(model.lock().unwrap().attempts, attempts);
    assert_eq!(attempts[&1], 1);

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    client.close().await.unwrap();
    bluez.close().await.unwrap();
}

#[tokio::test]
async fn private_bus_drains_signal_bursts_and_rejects_stale_snapshots() {
    let Some(bus) = TestBus::start() else {
        return;
    };
    let gate = Arc::new(tokio::sync::Notify::new());
    let model = Arc::new(Mutex::new(Model {
        adapters: vec![(1, true, true)],
        snapshot_gate: Some(gate.clone()),
        ..Default::default()
    }));
    let bluez = fake_bluez(&bus, &model).await;
    let client = bus.connect().await;
    let task_connection = client.clone();
    let task = tokio::spawn(async move {
        serve(&task_connection, Duration::from_secs(60))
            .await
            .unwrap();
    });
    until(|| model.lock().unwrap().snapshots == 1).await;
    model.lock().unwrap().adapters[0].1 = false;
    // Exceed zbus's default 64-message queue while GetManagedObjects is pending.
    for _ in 0..150 {
        bluez
            .send(&power(
                bluez.unique_name().unwrap().as_str(),
                1,
                Some(false),
            ))
            .await
            .unwrap();
    }
    gate.notify_one();
    until(|| model.lock().unwrap().snapshots >= 2).await;
    assert!(model.lock().unwrap().attempts.is_empty());
    model.lock().unwrap().adapters[0].1 = true;
    bluez
        .send(&power(bluez.unique_name().unwrap().as_str(), 1, Some(true)))
        .await
        .unwrap();
    until(|| model.lock().unwrap().registered.contains_key(&1)).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    client.close().await.unwrap();
    bluez.close().await.unwrap();
}

#[tokio::test]
async fn private_bus_removal_during_register_invalidates_late_success() {
    let Some(bus) = TestBus::start() else {
        return;
    };
    let gate = Arc::new(tokio::sync::Notify::new());
    let model = Arc::new(Mutex::new(Model {
        adapters: vec![(1, true, true)],
        register_gate: Some(gate.clone()),
        ..Default::default()
    }));
    let bluez = fake_bluez(&bus, &model).await;
    let client = bus.connect().await;
    let task_connection = client.clone();
    let task = tokio::spawn(async move {
        serve(&task_connection, Duration::from_secs(60))
            .await
            .unwrap();
    });
    until(|| model.lock().unwrap().registered.contains_key(&1)).await;
    model.lock().unwrap().registered.clear();
    bluez
        .send(&removed(bluez.unique_name().unwrap().as_str(), 1, ADAPTER))
        .await
        .unwrap();
    announce(&bluez, &model, 1).await;
    gate.notify_one();
    until(|| model.lock().unwrap().registered.contains_key(&1)).await;
    assert_eq!(model.lock().unwrap().attempts[&1], 2);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    client.close().await.unwrap();
    bluez.close().await.unwrap();
}
