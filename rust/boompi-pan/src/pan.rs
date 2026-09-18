use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{FutureExt, StreamExt};
use zbus::fdo::{DBusProxy, ManagedObjects, ObjectManagerProxy};
use zbus::message::{Message, Type};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const NETWORK: &str = "org.bluez.NetworkServer1";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DBUS: &str = "org.freedesktop.DBus";

#[derive(Default)]
struct State {
    owner: Option<String>,
    registered: HashSet<OwnedObjectPath>,
    // Detect removal/readdition at the same path while Register is in flight.
    generations: HashMap<OwnedObjectPath, u64>,
    revision: u64,
}

impl State {
    fn set_owner(&mut self, owner: Option<String>) {
        if self.owner != owner {
            self.owner = owner;
            self.registered.clear();
            self.generations.clear();
            self.revision += 1;
        }
    }

    fn generation(&self, path: &OwnedObjectPath) -> u64 {
        self.generations.get(path).copied().unwrap_or_default()
    }

    fn signal(&mut self, message: &Message) -> Result<()> {
        let header = message.header();
        if header.message_type() != Type::Signal {
            return Ok(());
        }
        let sender = header.sender().map(|name| name.as_str());
        let interface = header.interface().map(|name| name.as_str());
        let member = header.member().map(|name| name.as_str());
        if sender == Some(DBUS)
            && interface == Some(DBUS)
            && member == Some("NameOwnerChanged")
            && header.path().map(|path| path.as_str()) == Some("/org/freedesktop/DBus")
        {
            let (name, _, new): (String, String, String) = message.body().deserialize()?;
            if name == BLUEZ {
                self.set_owner((!new.is_empty()).then_some(new));
            }
            return Ok(());
        }
        if sender.is_none() || sender != self.owner.as_deref() {
            return Ok(());
        }
        match (interface, member) {
            (Some(OBJECT_MANAGER), Some("InterfacesAdded")) => {
                let (_, interfaces): (
                    OwnedObjectPath,
                    HashMap<String, HashMap<String, OwnedValue>>,
                ) = message.body().deserialize()?;
                if interfaces.contains_key(ADAPTER) || interfaces.contains_key(NETWORK) {
                    self.revision += 1;
                }
            }
            (Some(OBJECT_MANAGER), Some("InterfacesRemoved")) => {
                let (path, interfaces): (OwnedObjectPath, Vec<String>) =
                    message.body().deserialize()?;
                if interfaces.iter().any(|i| i == ADAPTER || i == NETWORK) {
                    self.registered.remove(&path);
                    *self.generations.entry(path).or_default() += 1;
                    self.revision += 1;
                }
            }
            (Some(PROPERTIES), Some("PropertiesChanged")) => {
                let (interface, changed, invalidated): (
                    String,
                    HashMap<String, OwnedValue>,
                    Vec<String>,
                ) = message.body().deserialize()?;
                if interface == ADAPTER
                    && (changed.contains_key("Powered")
                        || invalidated.iter().any(|name| name == "Powered"))
                {
                    // Power alone does not destroy BlueZ's registration. Keep
                    // ownership until the server/adapter interface disappears.
                    self.revision += 1;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn adapters(objects: &ManagedObjects) -> HashMap<OwnedObjectPath, bool> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interfaces.get(ADAPTER)?;
            interfaces.get(NETWORK)?;
            let powered = properties
                .get("Powered")
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false);
            Some((path.clone(), powered))
        })
        .collect()
}

// Poll the stream even during method calls: zbus applies backpressure rather
// than dropping signals, so awaiting a reply without draining can deadlock.
async fn with_signals<F: Future>(
    stream: &mut MessageStream,
    state: &mut State,
    future: F,
) -> Result<F::Output> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            message = stream.next() => {
                state.signal(&message.ok_or_else(|| anyhow!("D-Bus disconnected"))??)?;
            }
            result = &mut future => {
                // A reply can wake its caller before this stream has consumed
                // earlier signals. Apply those before accepting the snapshot.
                while let Some(message) = stream.next().now_or_never() {
                    state.signal(&message.ok_or_else(|| anyhow!("D-Bus disconnected"))??)?;
                }
                return Ok(result);
            },
        }
    }
}

async fn reconcile(
    connection: &Connection,
    bus: &DBusProxy<'_>,
    stream: &mut MessageStream,
    state: &mut State,
) -> Result<()> {
    let revision = state.revision;
    let owner = with_signals(stream, state, bus.get_name_owner(BLUEZ.try_into()?)).await?;
    if revision != state.revision {
        return Ok(());
    }
    match owner {
        Ok(owner) => state.set_owner(Some(owner.to_string())),
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => {
            state.set_owner(None);
            return Ok(());
        }
        Err(error) => {
            eprintln!("boompi-pan: looking up BlueZ owner: {error}; will retry");
            return Ok(());
        }
    }
    let owner = state.owner.clone().unwrap();
    // Pin all calls to this incarnation of BlueZ, never a replacement owner.
    let manager = ObjectManagerProxy::builder(connection)
        .destination(owner.as_str())?
        .path("/")?
        .build()
        .await?;
    let revision = state.revision;
    let objects = with_signals(stream, state, manager.get_managed_objects()).await?;
    if revision != state.revision {
        return Ok(());
    }
    let objects = match objects {
        Ok(objects) => objects,
        Err(error) => {
            eprintln!("boompi-pan: reading BlueZ adapters: {error}; will retry");
            return Ok(());
        }
    };
    let adapters = adapters(&objects);
    state.registered.retain(|path| adapters.contains_key(path));
    for (path, powered) in adapters {
        if !powered || state.registered.contains(&path) {
            continue;
        }
        // A signal during a previous call invalidates the remaining snapshot.
        if state.revision != revision {
            break;
        }
        let generation = state.generation(&path);
        let result = with_signals(
            stream,
            state,
            connection.call_method(
                Some(owner.as_str()),
                path.as_str(),
                Some(NETWORK),
                "Register",
                &("nap", "br-pan"),
            ),
        )
        .await?;
        match result {
            Ok(_) => {
                if state.owner.as_deref() == Some(owner.as_str())
                    && state.generation(&path) == generation
                {
                    state.registered.insert(path.clone());
                    eprintln!("boompi-pan: registered nap on {path} using br-pan");
                }
            }
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.bluez.Error.AlreadyExists" =>
            {
                // This can also follow a lost successful reply. The API cannot
                // prove who owns it; leave it untouched and retry on this same
                // connection. Closing/reopening could disconnect our clients.
                eprintln!("boompi-pan: NAP already exists on {path}; ownership unknown, leaving it untouched");
            }
            Err(error) => {
                eprintln!("boompi-pan: registering NAP on {path}: {error}; will retry");
            }
        }
    }
    Ok(())
}

pub(super) async fn serve(connection: &Connection, retry: Duration) -> Result<()> {
    // One ordered stream receives all three subscriptions. Install it before
    // AddMatch/GetNameOwner/GetManagedObjects to close the startup race.
    let mut stream = MessageStream::from(connection);
    let mut state = State::default();
    let bus = DBusProxy::new(connection).await?;
    let rules = [
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(DBUS)?
            .interface(DBUS)?
            .member("NameOwnerChanged")?
            .add_arg(BLUEZ)?
            .build(),
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(BLUEZ)?
            .interface(OBJECT_MANAGER)?
            .build(),
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(BLUEZ)?
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .add_arg(ADAPTER)?
            .build(),
    ];
    for rule in rules {
        with_signals(&mut stream, &mut state, bus.add_match_rule(rule)).await??;
    }
    let mut timer = tokio::time::interval(retry);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;
    loop {
        if dirty {
            let revision = state.revision;
            reconcile(connection, &bus, &mut stream, &mut state).await?;
            dirty = revision != state.revision;
        } else {
            tokio::select! {
                message = stream.next() => {
                    let revision = state.revision;
                    state.signal(&message.ok_or_else(|| anyhow!("D-Bus disconnected"))??)?;
                    dirty = revision != state.revision;
                }
                _ = timer.tick() => dirty = true,
            }
        }
    }
}

#[cfg(test)]
mod tests;
