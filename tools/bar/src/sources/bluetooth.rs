use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use zbus::fdo::ManagedObjects;
use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator, fdo::ObjectManagerProxy},
    message::Type,
    zvariant::OwnedValue,
};

use crate::{BluetoothState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

use super::{CancellableRecv, SourceSupervisor, forward_blocking_iterator, recv_with_cancellation};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BluetoothDevice {
    alias: String,
    address: String,
    connected: bool,
    audio_capable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusedHeadphones {
    alias: &'static str,
    mac: &'static str,
}

const BLUEZ_DESTINATION: &str = "org.bluez";
const BLUEZ_PATH_NAMESPACE: &str = "/org/bluez";
const BLUEZ_OBJECT_MANAGER_PATH: &str = "/";
const BLUEZ_ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
const BLUEZ_DEVICE_INTERFACE: &str = "org.bluez.Device1";
const BLUEZ_SIGNAL_QUEUE: usize = 32;
const BLUETOOTH_RESTART_DELAY: Duration = Duration::from_secs(1);

const HEADPHONES: FocusedHeadphones = FocusedHeadphones {
    alias: "Headphones",
    mac: "88:C9:E8:25:7B:04",
};

fn build_bluetooth_state(
    powered: bool,
    devices: &[BluetoothDevice],
    focused: &FocusedHeadphones,
) -> BluetoothState {
    if !powered {
        return BluetoothState {
            powered,
            connected_device: None,
            audio_device: None,
        };
    }

    let connected = devices
        .iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    let connected_device = match connected.len() {
        0 => None,
        1 => Some(connected[0].alias.clone()),
        count => Some(format!("{count} devices")),
    };

    let focused_mac = focused.mac.to_ascii_lowercase();
    let audio_device = connected
        .iter()
        .find(|device| device.address.to_ascii_lowercase() == focused_mac)
        .map(|_| focused.alias.to_string())
        .or_else(|| {
            connected
                .iter()
                .find(|device| {
                    device.audio_capable || device.alias.eq_ignore_ascii_case(focused.alias)
                })
                .map(|device| device.alias.clone())
        });

    BluetoothState {
        powered,
        connected_device,
        audio_device,
    }
}

fn publish_bluetooth_snapshot(
    sender: &Sender<StateUpdate>,
    cancelled: &Arc<AtomicBool>,
    connection: &Connection,
) -> Result<()> {
    let state = read_bluetooth_state(connection)?;
    if sender
        .send(StateUpdate::System(SystemUpdate::Bluetooth(state)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }

    Ok(())
}

fn read_bluetooth_state(connection: &Connection) -> Result<BluetoothState> {
    let object_manager =
        ObjectManagerProxy::new(connection, BLUEZ_DESTINATION, BLUEZ_OBJECT_MANAGER_PATH)
            .context("failed to build BlueZ object manager proxy")?;
    let managed = object_manager
        .get_managed_objects()
        .context("failed to read BlueZ managed objects")?;

    let (powered, devices) = parse_managed_objects(managed)?;
    Ok(build_bluetooth_state(powered, &devices, &HEADPHONES))
}

fn parse_managed_objects(managed: ManagedObjects) -> Result<(bool, Vec<BluetoothDevice>)> {
    let mut powered = None;
    let mut devices = Vec::new();

    for interfaces in managed.into_values() {
        if let Some(adapter) = interfaces.get(BLUEZ_ADAPTER_INTERFACE) {
            powered = Some(bool_property(adapter, "Powered").unwrap_or(false));
        }
        if let Some(device) = interfaces.get(BLUEZ_DEVICE_INTERFACE) {
            devices.push(BluetoothDevice {
                alias: string_property(device, "Alias")
                    .or_else(|| string_property(device, "Name"))
                    .unwrap_or_else(|| "Bluetooth device".to_string()),
                address: string_property(device, "Address").unwrap_or_default(),
                connected: bool_property(device, "Connected").unwrap_or(false),
                audio_capable: string_vec_property(device, "UUIDs")
                    .into_iter()
                    .flatten()
                    .any(|uuid| is_audio_uuid(&uuid)),
            });
        }
    }

    let Some(powered) = powered else {
        bail!("BlueZ did not expose an adapter");
    };

    devices.sort_by(|left, right| left.alias.cmp(&right.alias));
    Ok((powered, devices))
}

fn string_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = properties.get(key)?.clone();
    <String as TryFrom<OwnedValue>>::try_from(value).ok()
}

fn bool_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    let value = properties.get(key)?.clone();
    <bool as TryFrom<OwnedValue>>::try_from(value).ok()
}

fn string_vec_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<Vec<String>> {
    let value = properties.get(key)?.clone();
    <Vec<String> as TryFrom<OwnedValue>>::try_from(value).ok()
}

fn is_audio_uuid(uuid: &str) -> bool {
    matches!(
        uuid,
        "0000110b-0000-1000-8000-00805f9b34fb"
            | "0000110d-0000-1000-8000-00805f9b34fb"
            | "0000110e-0000-1000-8000-00805f9b34fb"
            | "00001108-0000-1000-8000-00805f9b34fb"
            | "0000111e-0000-1000-8000-00805f9b34fb"
            | "0000111f-0000-1000-8000-00805f9b34fb"
            | "0000110a-0000-1000-8000-00805f9b34fb"
    )
}

fn bluez_signal_stream(connection: &Connection) -> Result<MessageIterator> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(BLUEZ_DESTINATION)?
        .path_namespace(BLUEZ_PATH_NAMESPACE)?
        .build();
    MessageIterator::for_match_rule(rule, connection, Some(BLUEZ_SIGNAL_QUEUE))
        .context("failed to subscribe to BlueZ signals")
}

pub fn spawn_bluetooth_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    SourceSupervisor::spawn(cancelled.clone(), BLUETOOTH_RESTART_DELAY, move || {
        match run_bluetooth_worker(&sender, &cancelled) {
            Ok(healthy) => Ok(healthy),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Bluetooth,
                    health: SourceHealth::Disconnected {
                        message: error.to_string(),
                    },
                });
                Err(error)
            }
        }
    })
}

fn run_bluetooth_worker(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<bool> {
    let connection = Connection::system().context("failed to connect to system D-Bus for BlueZ")?;
    let signals = bluez_signal_stream(&connection)?;
    let signal_events = forward_blocking_iterator(signals);

    publish_bluetooth_snapshot(sender, cancelled, &connection)?;

    loop {
        match recv_with_cancellation(&signal_events, cancelled, Duration::from_millis(100)) {
            CancellableRecv::Item(Some(message)) => {
                message.context("failed to receive BlueZ signal")?;
                publish_bluetooth_snapshot(sender, cancelled, &connection)?;
            }
            CancellableRecv::Item(None) | CancellableRecv::Disconnected => {
                return Err(anyhow!("BlueZ signal stream closed unexpectedly"));
            }
            CancellableRecv::Cancelled => return Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BluetoothDevice, FocusedHeadphones, build_bluetooth_state};
    use crate::BluetoothState;

    const FOCUSED: FocusedHeadphones = FocusedHeadphones {
        alias: "Headphones",
        mac: "88:C9:E8:25:7B:04",
    };

    #[test]
    fn powered_off_adapter_clears_connected_devices() {
        assert_eq!(
            build_bluetooth_state(false, &[], &FOCUSED),
            BluetoothState {
                powered: false,
                connected_device: None,
                audio_device: None,
            }
        );
    }

    #[test]
    fn connected_devices_keep_generic_and_focused_audio_labels() {
        let state = build_bluetooth_state(
            true,
            &[
                BluetoothDevice {
                    alias: "Keychron K3".to_string(),
                    address: "AA:BB:CC:DD:EE:01".to_string(),
                    connected: true,
                    audio_capable: false,
                },
                BluetoothDevice {
                    alias: "WH-1000XM5".to_string(),
                    address: FOCUSED.mac.to_string(),
                    connected: true,
                    audio_capable: true,
                },
            ],
            &FOCUSED,
        );

        assert_eq!(
            state,
            BluetoothState {
                powered: true,
                connected_device: Some("2 devices".to_string()),
                audio_device: Some("Headphones".to_string()),
            }
        );
    }
}
