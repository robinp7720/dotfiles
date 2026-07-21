use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{RecvTimeoutError, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator, Proxy},
    message::Type,
    zvariant::OwnedObjectPath,
};

use crate::{ConnectivityState, NetworkState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

use super::{SourceSupervisor, forward_blocking_iterator};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveConnection {
    kind: ConnectionKind,
    label: Option<String>,
    signal_percent: Option<u8>,
    interface: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NetworkCapabilities {
    wifi: bool,
    ethernet: bool,
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
const DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";

const NETWORK_SIGNAL_QUEUE: usize = 32;
const NETWORK_RESTART_DELAY: Duration = Duration::from_secs(1);
const NETWORK_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const NETWORK_HISTORY_SAMPLES: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NetworkCounters {
    received_bytes: u64,
    transmitted_bytes: u64,
}

#[derive(Debug, Default)]
struct TrafficSampler {
    interface: Option<String>,
    previous: Option<(Instant, NetworkCounters)>,
    download_bytes_per_second: Option<u64>,
    upload_bytes_per_second: Option<u64>,
    download_history: VecDeque<u64>,
    upload_history: VecDeque<u64>,
}

impl TrafficSampler {
    fn update(&mut self, state: &mut NetworkState, sample: bool) {
        if self.interface != state.interface {
            self.reset(state.interface.clone());
        }
        if sample {
            self.sample();
        }
        state.download_bytes_per_second = self.download_bytes_per_second;
        state.upload_bytes_per_second = self.upload_bytes_per_second;
        state.download_history = self.download_history.iter().copied().collect();
        state.upload_history = self.upload_history.iter().copied().collect();
    }

    fn sample(&mut self) {
        let Some(interface) = self.interface.as_deref() else {
            return;
        };
        let Ok(counters) = read_network_counters(interface) else {
            self.download_bytes_per_second = None;
            self.upload_bytes_per_second = None;
            return;
        };
        self.record(Instant::now(), counters);
    }

    fn record(&mut self, now: Instant, counters: NetworkCounters) {
        if let Some((previous_at, previous)) = self.previous {
            let elapsed = now.duration_since(previous_at).as_secs_f64();
            if elapsed > 0.0 {
                let download = ((counters
                    .received_bytes
                    .saturating_sub(previous.received_bytes)
                    as f64)
                    / elapsed)
                    .round() as u64;
                let upload = ((counters
                    .transmitted_bytes
                    .saturating_sub(previous.transmitted_bytes)
                    as f64)
                    / elapsed)
                    .round() as u64;
                self.download_bytes_per_second = Some(download);
                self.upload_bytes_per_second = Some(upload);
                push_sample(&mut self.download_history, download);
                push_sample(&mut self.upload_history, upload);
            }
        }
        self.previous = Some((now, counters));
    }

    fn reset(&mut self, interface: Option<String>) {
        self.interface = interface;
        self.previous = None;
        self.download_bytes_per_second = None;
        self.upload_bytes_per_second = None;
        self.download_history.clear();
        self.upload_history.clear();
    }
}

fn push_sample(history: &mut VecDeque<u64>, sample: u64) {
    if history.len() == NETWORK_HISTORY_SAMPLES {
        history.pop_front();
    }
    history.push_back(sample);
}

fn read_network_counters(interface: &str) -> Result<NetworkCounters> {
    let statistics = Path::new("/sys/class/net")
        .join(interface)
        .join("statistics");
    Ok(NetworkCounters {
        received_bytes: read_counter(&statistics.join("rx_bytes"))?,
        transmitted_bytes: read_counter(&statistics.join("tx_bytes"))?,
    })
}

fn read_counter(path: &Path) -> Result<u64> {
    fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid counter in {}", path.display()))
}

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
    capabilities: NetworkCapabilities,
    active: Option<&ActiveConnection>,
) -> NetworkState {
    let connectivity = map_connectivity_state(connectivity);

    let mut state = match active {
        Some(active) => match active.kind {
            ConnectionKind::Wifi => NetworkState {
                connectivity,
                icon_hint: Some(wifi_signal_icon(active.signal_percent).to_string()),
                label: active.label.clone(),
                wifi_available: capabilities.wifi,
                ethernet_available: capabilities.ethernet,
                wifi_enabled,
                ..NetworkState::default()
            },
            ConnectionKind::Ethernet => NetworkState {
                connectivity,
                icon_hint: Some("network-wired-symbolic".to_string()),
                label: active
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| Some("Ethernet".to_string())),
                wifi_available: capabilities.wifi,
                ethernet_available: capabilities.ethernet,
                wifi_enabled,
                ..NetworkState::default()
            },
            ConnectionKind::Other => NetworkState {
                connectivity,
                icon_hint: Some("network-idle-symbolic".to_string()),
                label: active.label.clone(),
                wifi_available: capabilities.wifi,
                ethernet_available: capabilities.ethernet,
                wifi_enabled,
                ..NetworkState::default()
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
            wifi_available: capabilities.wifi,
            ethernet_available: capabilities.ethernet,
            wifi_enabled,
            ..NetworkState::default()
        },
    };
    state.interface = active.and_then(|connection| connection.interface.clone());
    state
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
    state: &NetworkState,
) -> Result<()> {
    if sender
        .send(StateUpdate::System(SystemUpdate::Network(state.clone())))
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
    let capabilities = read_network_capabilities(connection, &manager)
        .or_else(|_| read_network_capabilities_nmcli())?;
    let wifi_enabled = capabilities
        .wifi
        .then(|| manager.get_property::<bool>("WirelessEnabled").ok())
        .flatten();
    let primary: OwnedObjectPath = manager
        .get_property("PrimaryConnection")
        .context("failed to read NetworkManager PrimaryConnection")?;
    let active = read_active_connection(connection, primary.as_str())
        .or_else(|_| read_network_state_nmcli())
        .ok();

    Ok(build_network_state(
        connectivity,
        wifi_enabled,
        capabilities,
        active.as_ref(),
    ))
}

fn read_network_capabilities(
    connection: &Connection,
    manager: &Proxy<'_>,
) -> Result<NetworkCapabilities> {
    let devices: Vec<OwnedObjectPath> = manager
        .call("GetDevices", &())
        .context("failed to query NetworkManager devices")?;
    let mut device_types = Vec::with_capacity(devices.len());
    for path in devices {
        let device = Proxy::new(
            connection,
            NETWORK_MANAGER_DESTINATION,
            path.as_str(),
            DEVICE_INTERFACE,
        )
        .with_context(|| format!("failed to build NetworkManager device proxy for {path}"))?;
        device_types.push(
            device
                .get_property::<u32>("DeviceType")
                .with_context(|| format!("failed to read NetworkManager device type for {path}"))?,
        );
    }
    Ok(capabilities_from_device_types(&device_types))
}

fn capabilities_from_device_types(device_types: &[u32]) -> NetworkCapabilities {
    NetworkCapabilities {
        ethernet: device_types.contains(&1),
        wifi: device_types.contains(&2),
    }
}

fn read_network_capabilities_nmcli() -> Result<NetworkCapabilities> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "TYPE", "device", "status"])
        .output()
        .context("failed to execute nmcli for device capabilities")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("nmcli device capability query failed: {stderr}");
    }
    let stdout = String::from_utf8(output.stdout).context("nmcli output was not UTF-8")?;
    Ok(parse_nmcli_capabilities(&stdout))
}

fn parse_nmcli_capabilities(text: &str) -> NetworkCapabilities {
    NetworkCapabilities {
        ethernet: text.lines().any(|line| line == "ethernet"),
        wifi: text.lines().any(|line| line == "wifi"),
    }
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
    let interface = read_active_interface(connection, &proxy).ok();

    if kind != ConnectionKind::Wifi {
        return Ok(ActiveConnection {
            kind,
            label: if label.trim().is_empty() {
                None
            } else {
                Some(label)
            },
            signal_percent: None,
            interface,
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
        interface,
    })
}

fn read_active_interface(connection: &Connection, active: &Proxy<'_>) -> Result<String> {
    let devices: Vec<OwnedObjectPath> = active
        .get_property("Devices")
        .context("failed to read active connection devices")?;
    let device = devices
        .first()
        .context("active connection did not expose a device")?;
    Proxy::new(
        connection,
        NETWORK_MANAGER_DESTINATION,
        device.as_str(),
        DEVICE_INTERFACE,
    )
    .with_context(|| format!("failed to build NetworkManager device proxy for {device}"))?
    .get_property::<String>("Interface")
    .context("failed to read active network interface")
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
            "DEVICE,TYPE,STATE,CONNECTION,SIGNAL",
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
        let interface = normalize_label(fields.next().unwrap_or_default());
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
            interface,
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

    let mut state = read_network_state(&connection)?;
    let mut sampler = TrafficSampler::default();
    sampler.update(&mut state, true);
    publish_network_snapshot(sender, cancelled, &state)?;
    let mut next_sample = Instant::now() + NETWORK_SAMPLE_INTERVAL;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        match signal_events.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(message)) => {
                message.context("failed to receive NetworkManager signal")?;
                state = read_network_state(&connection)?;
                sampler.update(&mut state, false);
                publish_network_snapshot(sender, cancelled, &state)?;
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("NetworkManager signal stream closed unexpectedly"));
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        let now = Instant::now();
        if now >= next_sample {
            sampler.update(&mut state, true);
            publish_network_snapshot(sender, cancelled, &state)?;
            next_sample = now + NETWORK_SAMPLE_INTERVAL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveConnection, ConnectionKind, NETWORK_HISTORY_SAMPLES, NetworkCapabilities,
        NetworkCounters, TrafficSampler, build_network_state, capabilities_from_device_types,
        map_connectivity_state, parse_nmcli_capabilities, push_sample,
    };
    use crate::{ConnectivityState, NetworkState};
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

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
            NetworkCapabilities {
                wifi: true,
                ethernet: true,
            },
            Some(&ActiveConnection {
                kind: ConnectionKind::Wifi,
                label: Some("Cafe | Wi-Fi".to_string()),
                signal_percent: Some(78),
                interface: Some("wlan0".to_string()),
            }),
        );

        assert_eq!(
            state,
            NetworkState {
                connectivity: ConnectivityState::Connected,
                icon_hint: Some("network-wireless-signal-good-symbolic".to_string()),
                label: Some("Cafe | Wi-Fi".to_string()),
                wifi_available: true,
                ethernet_available: true,
                wifi_enabled: Some(true),
                interface: Some("wlan0".to_string()),
                ..NetworkState::default()
            }
        );
    }

    #[test]
    fn wifi_radio_state_is_preserved_without_an_active_connection() {
        let state = build_network_state(
            1,
            Some(false),
            NetworkCapabilities {
                wifi: true,
                ethernet: false,
            },
            None,
        );

        assert_eq!(state.wifi_enabled, Some(false));
        assert_eq!(state.connectivity, ConnectivityState::Disconnected);
        assert_eq!(state.interface, None);
    }

    #[test]
    fn device_types_distinguish_desktop_and_laptop_network_hardware() {
        assert_eq!(
            capabilities_from_device_types(&[1, 32]),
            NetworkCapabilities {
                wifi: false,
                ethernet: true,
            }
        );
        assert_eq!(
            capabilities_from_device_types(&[1, 2]),
            NetworkCapabilities {
                wifi: true,
                ethernet: true,
            }
        );
        assert_eq!(
            capabilities_from_device_types(&[]),
            NetworkCapabilities::default()
        );
    }

    #[test]
    fn nmcli_capability_fallback_reads_all_devices_not_only_active_ones() {
        assert_eq!(
            parse_nmcli_capabilities("ethernet\nwifi\nloopback\n"),
            NetworkCapabilities {
                wifi: true,
                ethernet: true,
            }
        );
    }

    #[test]
    fn network_history_keeps_only_the_latest_minute() {
        let mut history = VecDeque::new();
        for sample in 0..(NETWORK_HISTORY_SAMPLES as u64 + 3) {
            push_sample(&mut history, sample);
        }

        assert_eq!(history.len(), NETWORK_HISTORY_SAMPLES);
        assert_eq!(history.front(), Some(&3));
        assert_eq!(history.back(), Some(&(NETWORK_HISTORY_SAMPLES as u64 + 2)));
    }

    #[test]
    fn traffic_sampler_calculates_rates_from_counter_deltas() {
        let start = Instant::now();
        let mut sampler = TrafficSampler::default();
        sampler.reset(Some("eth0".to_string()));
        sampler.record(
            start,
            NetworkCounters {
                received_bytes: 1_000,
                transmitted_bytes: 2_000,
            },
        );
        sampler.record(
            start + Duration::from_secs(2),
            NetworkCounters {
                received_bytes: 5_096,
                transmitted_bytes: 4_048,
            },
        );

        assert_eq!(sampler.download_bytes_per_second, Some(2_048));
        assert_eq!(sampler.upload_bytes_per_second, Some(1_024));
        assert_eq!(sampler.download_history.back(), Some(&2_048));
        assert_eq!(sampler.upload_history.back(), Some(&1_024));
    }
}
