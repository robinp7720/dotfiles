use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator, Proxy},
    message::Type,
    zvariant::OwnedObjectPath,
};

use crate::{ConnectivityState, NetworkState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

use super::{CancellableRecv, SourceSupervisor, forward_blocking_iterator, recv_with_cancellation};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveConnection {
    kind: ConnectionKind,
    label: Option<String>,
    signal_percent: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectionKind {
    Ethernet,
    Wifi,
    Other,
}

const NETWORK_MANAGER_DESTINATION: &str = "org.freedesktop.NetworkManager";
const NETWORK_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager";
const NETWORK_MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
const ACTIVE_CONNECTION_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

const NETWORK_SIGNAL_QUEUE: usize = 32;
const NETWORK_RESTART_DELAY: Duration = Duration::from_secs(1);

fn map_connectivity_state(value: u32) -> ConnectivityState {
    match value {
        1 => ConnectivityState::Disconnected,
        2 | 3 => ConnectivityState::Connecting,
        4 => ConnectivityState::Connected,
        _ => ConnectivityState::Unknown,
    }
}

fn build_network_state(
    connectivity: u32,
    wifi_enabled: Option<bool>,
    active: Option<&ActiveConnection>,
) -> NetworkState {
    let connectivity = map_connectivity_state(connectivity);

    match active {
        Some(active) => match active.kind {
            ConnectionKind::Wifi => NetworkState {
                connectivity,
                icon_hint: Some(wifi_signal_icon(active.signal_percent).to_string()),
                label: active.label.clone(),
                wifi_enabled,
            },
            ConnectionKind::Ethernet => NetworkState {
                connectivity,
                icon_hint: Some("network-wired-symbolic".to_string()),
                label: active
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| Some("Ethernet".to_string())),
                wifi_enabled,
            },
            ConnectionKind::Other => NetworkState {
                connectivity,
                icon_hint: Some("network-idle-symbolic".to_string()),
                label: active.label.clone(),
                wifi_enabled,
            },
        },
        None => NetworkState {
            connectivity: connectivity.clone(),
            icon_hint: Some(
                match connectivity {
                    ConnectivityState::Unknown => "network-idle-symbolic",
                    ConnectivityState::Disconnected => "network-offline-symbolic",
                    ConnectivityState::Connecting => "network-transmit-receive-symbolic",
                    ConnectivityState::Connected => "network-idle-symbolic",
                }
                .to_string(),
            ),
            label: None,
            wifi_enabled,
        },
    }
}

fn wifi_signal_icon(signal_percent: Option<u8>) -> &'static str {
    match signal_percent.unwrap_or_default() {
        80..=100 => "network-wireless-signal-excellent-symbolic",
        55..=79 => "network-wireless-signal-good-symbolic",
        30..=54 => "network-wireless-signal-ok-symbolic",
        1..=29 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
}

fn publish_network_snapshot(
    sender: &Sender<StateUpdate>,
    cancelled: &Arc<AtomicBool>,
    connection: &Connection,
) -> Result<()> {
    let state = read_network_state(connection)?;
    if sender
        .send(StateUpdate::System(SystemUpdate::Network(state)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }

    Ok(())
}

fn read_network_state(connection: &Connection) -> Result<NetworkState> {
    let manager = Proxy::new(
        connection,
        NETWORK_MANAGER_DESTINATION,
        NETWORK_MANAGER_PATH,
        NETWORK_MANAGER_INTERFACE,
    )
    .context("failed to build NetworkManager proxy")?;
    let connectivity: u32 = manager
        .get_property("Connectivity")
        .context("failed to read NetworkManager Connectivity")?;
    let wifi_enabled = manager.get_property::<bool>("WirelessEnabled").ok();
    let primary: OwnedObjectPath = manager
        .get_property("PrimaryConnection")
        .context("failed to read NetworkManager PrimaryConnection")?;
    let active = read_active_connection(connection, primary.as_str())
        .or_else(|_| read_network_state_nmcli())
        .ok();

    Ok(build_network_state(
        connectivity,
        wifi_enabled,
        active.as_ref(),
    ))
}

fn read_active_connection(connection: &Connection, path: &str) -> Result<ActiveConnection> {
    if path == "/" {
        bail!("NetworkManager does not have a primary connection");
    }

    let proxy = Proxy::new(
        connection,
        NETWORK_MANAGER_DESTINATION,
        path,
        ACTIVE_CONNECTION_INTERFACE,
    )
    .with_context(|| {
        format!("failed to build NetworkManager active-connection proxy for {path}")
    })?;
    let kind = map_connection_kind(
        &proxy
            .get_property::<String>("Type")
            .context("failed to read active connection Type")?,
    );
    let label = proxy
        .get_property::<String>("Id")
        .context("failed to read active connection Id")?;

    if kind != ConnectionKind::Wifi {
        return Ok(ActiveConnection {
            kind,
            label: if label.trim().is_empty() {
                None
            } else {
                Some(label)
            },
            signal_percent: None,
        });
    }

    let specific: OwnedObjectPath = proxy
        .get_property("SpecificObject")
        .context("failed to read active connection SpecificObject")?;
    let (ssid, signal_percent) =
        read_access_point(connection, specific.as_str()).unwrap_or((normalize_label(&label), None));

    Ok(ActiveConnection {
        kind,
        label: ssid.or_else(|| normalize_label(&label)),
        signal_percent,
    })
}

fn read_access_point(connection: &Connection, path: &str) -> Result<(Option<String>, Option<u8>)> {
    if path == "/" {
        bail!("wireless connection does not expose an access point path");
    }

    let proxy = Proxy::new(
        connection,
        NETWORK_MANAGER_DESTINATION,
        path,
        ACCESS_POINT_INTERFACE,
    )
    .with_context(|| format!("failed to build NetworkManager access-point proxy for {path}"))?;
    let ssid: Vec<u8> = proxy
        .get_property("Ssid")
        .context("failed to read access point SSID")?;
    let strength: u8 = proxy
        .get_property("Strength")
        .context("failed to read access point Strength")?;

    Ok((decode_ssid(&ssid), Some(strength)))
}

fn read_network_state_nmcli() -> Result<ActiveConnection> {
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "TYPE,STATE,CONNECTION,SIGNAL",
            "device",
            "status",
        ])
        .output()
        .context("failed to execute nmcli for active connection fallback")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("nmcli device status failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("nmcli output was not UTF-8")?;
    for line in stdout.lines() {
        let mut fields = line.split(':');
        let kind = match fields.next() {
            Some("wifi") => ConnectionKind::Wifi,
            Some("ethernet") => ConnectionKind::Ethernet,
            Some(_) => ConnectionKind::Other,
            None => continue,
        };
        let state = fields.next().unwrap_or_default();
        if state != "connected" {
            continue;
        }

        let label = normalize_label(fields.next().unwrap_or_default());
        let signal_percent = fields.next().and_then(|field| field.parse::<u8>().ok());
        return Ok(ActiveConnection {
            kind,
            label,
            signal_percent,
        });
    }

    bail!("nmcli did not report an active network connection")
}

fn map_connection_kind(value: &str) -> ConnectionKind {
    match value {
        "802-3-ethernet" => ConnectionKind::Ethernet,
        "802-11-wireless" => ConnectionKind::Wifi,
        _ => ConnectionKind::Other,
    }
}

fn normalize_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn decode_ssid(bytes: &[u8]) -> Option<String> {
    let trimmed = bytes
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    let text = String::from_utf8(trimmed).ok()?;
    normalize_label(&text)
}

fn network_signal_stream(connection: &Connection) -> Result<MessageIterator> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(NETWORK_MANAGER_DESTINATION)?
        .path_namespace(NETWORK_MANAGER_PATH)?
        .build();
    MessageIterator::for_match_rule(rule, connection, Some(NETWORK_SIGNAL_QUEUE))
        .context("failed to subscribe to NetworkManager signals")
}

pub fn spawn_network_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    SourceSupervisor::spawn(cancelled.clone(), NETWORK_RESTART_DELAY, move || {
        match run_network_worker(&sender, &cancelled) {
            Ok(healthy) => Ok(healthy),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Network,
                    health: SourceHealth::Disconnected {
                        message: error.to_string(),
                    },
                });
                Err(error)
            }
        }
    })
}

fn run_network_worker(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<bool> {
    let connection =
        Connection::system().context("failed to connect to system D-Bus for NetworkManager")?;
    let signals = network_signal_stream(&connection)?;
    let signal_events = forward_blocking_iterator(signals);

    publish_network_snapshot(sender, cancelled, &connection)?;

    loop {
        match recv_with_cancellation(&signal_events, cancelled, Duration::from_millis(100)) {
            CancellableRecv::Item(Some(message)) => {
                message.context("failed to receive NetworkManager signal")?;
                publish_network_snapshot(sender, cancelled, &connection)?;
            }
            CancellableRecv::Item(None) | CancellableRecv::Disconnected => {
                return Err(anyhow!("NetworkManager signal stream closed unexpectedly"));
            }
            CancellableRecv::Cancelled => return Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ActiveConnection, ConnectionKind, build_network_state, map_connectivity_state};
    use crate::{ConnectivityState, NetworkState};

    #[test]
    fn networkmanager_connectivity_values_map_to_bar_states() {
        assert_eq!(map_connectivity_state(0), ConnectivityState::Unknown);
        assert_eq!(map_connectivity_state(1), ConnectivityState::Disconnected);
        assert_eq!(map_connectivity_state(2), ConnectivityState::Connecting);
        assert_eq!(map_connectivity_state(3), ConnectivityState::Connecting);
        assert_eq!(map_connectivity_state(4), ConnectivityState::Connected);
        assert_eq!(map_connectivity_state(99), ConnectivityState::Unknown);
    }

    #[test]
    fn wifi_connections_preserve_ssid_and_signal_bucket() {
        let state = build_network_state(
            4,
            Some(true),
            Some(&ActiveConnection {
                kind: ConnectionKind::Wifi,
                label: Some("Cafe | Wi-Fi".to_string()),
                signal_percent: Some(78),
            }),
        );

        assert_eq!(
            state,
            NetworkState {
                connectivity: ConnectivityState::Connected,
                icon_hint: Some("network-wireless-signal-good-symbolic".to_string()),
                label: Some("Cafe | Wi-Fi".to_string()),
                wifi_enabled: Some(true),
            }
        );
    }

    #[test]
    fn wifi_radio_state_is_preserved_without_an_active_connection() {
        let state = build_network_state(1, Some(false), None);

        assert_eq!(state.wifi_enabled, Some(false));
        assert_eq!(state.connectivity, ConnectivityState::Disconnected);
    }
}
