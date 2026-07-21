use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, Sender},
};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use zbus::fdo::ManagedObjects;
use zbus::{
    DBusError, MatchRule,
    blocking::{Connection, MessageIterator, Proxy, fdo::ObjectManagerProxy},
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::{
    BluetoothDeviceOperation, BluetoothDeviceState, BluetoothPairingPrompt,
    BluetoothPairingPromptKind, BluetoothPairingResponse, BluetoothState, SourceHealth, SourceId,
    StateUpdate, SystemUpdate,
};

use super::forward_blocking_iterator;

const BLUEZ_DESTINATION: &str = "org.bluez";
const BLUEZ_PATH_NAMESPACE: &str = "/org/bluez";
const BLUEZ_OBJECT_MANAGER_PATH: &str = "/";
const BLUEZ_AGENT_MANAGER_PATH: &str = "/org/bluez";
const BLUEZ_AGENT_INTERFACE: &str = "org.bluez.AgentManager1";
const BLUEZ_ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
const BLUEZ_DEVICE_INTERFACE: &str = "org.bluez.Device1";
const BLUEZ_BATTERY_INTERFACE: &str = "org.bluez.Battery1";
const BAR_AGENT_PATH: &str = "/org/cockpit_bar/BluetoothAgent";
const BLUETOOTH_RESTART_DELAY: Duration = Duration::from_secs(1);
const CONTROLLER_POLL: Duration = Duration::from_millis(80);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(60);
const NEARBY_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BluetoothCommand {
    SetPowered(bool),
    SetDiscovery(bool),
    Connect(String),
    Disconnect(String),
    Pair(String),
    Forget(String),
    RespondPairing {
        prompt_id: u64,
        response: BluetoothPairingResponse,
    },
    CancelPairing(String),
}

#[derive(Clone)]
pub struct BluetoothControlClient {
    sender: Sender<BluetoothCommand>,
}

impl BluetoothControlClient {
    pub fn send(&self, command: BluetoothCommand) -> Result<()> {
        self.sender
            .send(command)
            .context("Bluetooth controller is unavailable")
    }
}

#[derive(Clone, Debug)]
struct DeviceRecord {
    path: String,
    address: String,
    name: String,
    icon_name: String,
    paired: bool,
    trusted: bool,
    connected: bool,
    audio_capable: bool,
    battery_percent: Option<u8>,
    rssi: Option<i16>,
}

#[derive(Default)]
struct ParsedBluetoothState {
    available: bool,
    powered: bool,
    discovering: bool,
    adapter_path: Option<String>,
    devices: Vec<DeviceRecord>,
}

#[derive(Debug)]
enum InternalEvent {
    PromptOpened(BluetoothPairingPrompt),
    PromptClosed(u64),
    OperationFinished {
        address: String,
        result: Result<(), String>,
    },
}

struct PendingPairing {
    id: u64,
    address: String,
    response: Sender<BluetoothPairingResponse>,
}

struct PairingBroker {
    next_id: AtomicU64,
    pending: Mutex<Option<PendingPairing>>,
    active_pairs: Mutex<HashSet<String>>,
    events: Sender<InternalEvent>,
}

impl PairingBroker {
    fn new(events: Sender<InternalEvent>) -> Self {
        Self {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(None),
            active_pairs: Mutex::new(HashSet::new()),
            events,
        }
    }

    fn begin_pairing(&self, address: &str) {
        self.active_pairs
            .lock()
            .unwrap()
            .insert(address.to_string());
    }

    fn finish_pairing(&self, address: &str) {
        self.active_pairs.lock().unwrap().remove(address);
        self.reject_address(address);
    }

    fn is_active(&self, address: &str) -> bool {
        self.active_pairs.lock().unwrap().contains(address)
    }

    fn ask(
        &self,
        address: String,
        kind: BluetoothPairingPromptKind,
    ) -> Result<BluetoothPairingResponse, AgentError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        if let Some(previous) = self.pending.lock().unwrap().replace(PendingPairing {
            id,
            address: address.clone(),
            response: sender,
        }) {
            let _ = previous.response.send(BluetoothPairingResponse::Reject);
            let _ = self.events.send(InternalEvent::PromptClosed(previous.id));
        }
        let _ = self
            .events
            .send(InternalEvent::PromptOpened(BluetoothPairingPrompt {
                id,
                address,
                device_name: String::new(),
                kind,
            }));
        let result = receiver.recv_timeout(PAIRING_TIMEOUT);
        self.pending
            .lock()
            .unwrap()
            .take_if(|pending| pending.id == id);
        let _ = self.events.send(InternalEvent::PromptClosed(id));
        match result {
            Ok(BluetoothPairingResponse::Reject) => {
                Err(AgentError::Rejected("Pairing rejected".to_string()))
            }
            Ok(response) => Ok(response),
            Err(_) => Err(AgentError::Canceled("Pairing prompt timed out".to_string())),
        }
    }

    fn display(&self, address: String, kind: BluetoothPairingPromptKind) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .events
            .send(InternalEvent::PromptOpened(BluetoothPairingPrompt {
                id,
                address,
                device_name: String::new(),
                kind,
            }));
    }

    fn respond(&self, prompt_id: u64, response: BluetoothPairingResponse) -> Result<()> {
        let pending = self
            .pending
            .lock()
            .unwrap()
            .take_if(|pending| pending.id == prompt_id);
        let Some(pending) = pending else {
            bail!("pairing prompt is no longer active");
        };
        pending
            .response
            .send(response)
            .context("pairing request is no longer active")
    }

    fn reject_address(&self, address: &str) {
        let pending = self
            .pending
            .lock()
            .unwrap()
            .take_if(|pending| pending.address == address);
        if let Some(pending) = pending {
            let _ = pending.response.send(BluetoothPairingResponse::Reject);
            let _ = self.events.send(InternalEvent::PromptClosed(pending.id));
        }
    }

    fn cancel_current(&self) {
        if let Some(pending) = self.pending.lock().unwrap().take() {
            let _ = pending.response.send(BluetoothPairingResponse::Reject);
            let _ = self.events.send(InternalEvent::PromptClosed(pending.id));
        }
    }
}

#[derive(Debug, DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    Rejected(String),
    Canceled(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

struct PairingAgent {
    broker: Arc<PairingBroker>,
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl PairingAgent {
    fn release(&self) {
        self.broker.cancel_current();
    }

    fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, AgentError> {
        let response = self.broker.ask(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::EnterPinCode,
        )?;
        match response {
            BluetoothPairingResponse::PinCode(value)
                if !value.is_empty()
                    && value.len() <= 16
                    && value
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric()) =>
            {
                Ok(value)
            }
            _ => Err(AgentError::Rejected("A valid PIN is required".to_string())),
        }
    }

    fn display_pin_code(&self, device: OwnedObjectPath, pin_code: String) {
        self.broker.display(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::DisplayPinCode { pin_code },
        );
    }

    fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, AgentError> {
        match self.broker.ask(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::EnterPasskey,
        )? {
            BluetoothPairingResponse::Passkey(value) if value <= 999_999 => Ok(value),
            _ => Err(AgentError::Rejected(
                "A valid passkey is required".to_string(),
            )),
        }
    }

    fn display_passkey(&self, device: OwnedObjectPath, passkey: u32, entered: u16) {
        self.broker.display(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::DisplayPasskey { passkey, entered },
        );
    }

    fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
    ) -> Result<(), AgentError> {
        match self.broker.ask(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::ConfirmPasskey { passkey },
        )? {
            BluetoothPairingResponse::Accept => Ok(()),
            _ => Err(AgentError::Rejected(
                "Passkey was not confirmed".to_string(),
            )),
        }
    }

    fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
        match self.broker.ask(
            address_from_path(device.as_str()),
            BluetoothPairingPromptKind::Authorize,
        )? {
            BluetoothPairingResponse::Accept => Ok(()),
            _ => Err(AgentError::Rejected(
                "Pairing was not authorized".to_string(),
            )),
        }
    }

    fn authorize_service(&self, device: OwnedObjectPath, _uuid: String) -> Result<(), AgentError> {
        let address = address_from_path(device.as_str());
        if self.broker.is_active(&address) {
            Ok(())
        } else {
            Err(AgentError::Rejected(
                "No pairing initiated by cockpit-bar is active".to_string(),
            ))
        }
    }

    fn cancel(&self) {
        self.broker.cancel_current();
    }
}

struct ControllerState<'a> {
    connection: Connection,
    state_sender: Sender<StateUpdate>,
    commands: &'a Receiver<BluetoothCommand>,
    signal_events: Receiver<Option<Result<zbus::Message, zbus::Error>>>,
    internal_events: Receiver<InternalEvent>,
    internal_sender: Sender<InternalEvent>,
    broker: Arc<PairingBroker>,
    nearby_seen: HashMap<String, Instant>,
    operations: BTreeMap<String, BluetoothDeviceOperation>,
    errors: BTreeMap<String, String>,
    pairing_prompt: Option<BluetoothPairingPrompt>,
    top_error: Option<String>,
    adapter_path: Option<String>,
    device_paths: BTreeMap<String, String>,
    discovery_requested: bool,
}

pub fn spawn_bluetooth_source(
    state_sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> (BluetoothControlClient, thread::JoinHandle<()>) {
    let (command_sender, command_receiver) = mpsc::channel();
    let client = BluetoothControlClient {
        sender: command_sender,
    };
    let handle = thread::spawn(move || {
        while !cancelled.load(Ordering::Relaxed) {
            match run_bluetooth_controller(&state_sender, &command_receiver, &cancelled) {
                Ok(()) if cancelled.load(Ordering::Relaxed) => break,
                Ok(()) => {}
                Err(error) => {
                    let _ = state_sender.send(StateUpdate::Health {
                        source: SourceId::Bluetooth,
                        health: SourceHealth::Disconnected {
                            message: error.to_string(),
                        },
                    });
                }
            }
            if !cancelled.load(Ordering::Relaxed) {
                thread::sleep(BLUETOOTH_RESTART_DELAY);
            }
        }
    });
    (client, handle)
}

fn run_bluetooth_controller(
    state_sender: &Sender<StateUpdate>,
    commands: &Receiver<BluetoothCommand>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let connection = Connection::system().context("failed to connect to system D-Bus for BlueZ")?;
    let (internal_sender, internal_events) = mpsc::channel();
    let broker = Arc::new(PairingBroker::new(internal_sender.clone()));
    connection
        .object_server()
        .at(
            BAR_AGENT_PATH,
            PairingAgent {
                broker: broker.clone(),
            },
        )
        .context("failed to export Bluetooth pairing agent")?;
    let agent_manager = Proxy::new(
        &connection,
        BLUEZ_DESTINATION,
        BLUEZ_AGENT_MANAGER_PATH,
        BLUEZ_AGENT_INTERFACE,
    )?;
    let agent_path = OwnedObjectPath::try_from(BAR_AGENT_PATH)?;
    agent_manager
        .call::<_, _, ()>("RegisterAgent", &(agent_path, "KeyboardDisplay"))
        .context("failed to register Bluetooth pairing agent")?;

    let signals = bluez_signal_stream(&connection)?;
    let signal_events = forward_blocking_iterator(signals);
    let mut controller = ControllerState {
        connection,
        state_sender: state_sender.clone(),
        commands,
        signal_events,
        internal_events,
        internal_sender,
        broker,
        nearby_seen: HashMap::new(),
        operations: BTreeMap::new(),
        errors: BTreeMap::new(),
        pairing_prompt: None,
        top_error: None,
        adapter_path: None,
        device_paths: BTreeMap::new(),
        discovery_requested: false,
    };
    controller.publish_snapshot()?;

    while !cancelled.load(Ordering::Relaxed) {
        while let Ok(command) = controller.commands.try_recv() {
            controller.handle_command(command);
        }
        while let Ok(event) = controller.internal_events.try_recv() {
            controller.handle_internal_event(event);
        }
        match controller.signal_events.recv_timeout(CONTROLLER_POLL) {
            Ok(Some(Ok(message))) => {
                controller.note_signal(&message);
                controller.publish_snapshot()?;
            }
            Ok(Some(Err(error))) => return Err(error).context("failed to receive BlueZ signal"),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("BlueZ signal stream closed unexpectedly"));
            }
            Err(RecvTimeoutError::Timeout) => {
                let before = controller.nearby_seen.len();
                controller
                    .nearby_seen
                    .retain(|_, seen| seen.elapsed() <= NEARBY_TTL);
                if controller.nearby_seen.len() != before {
                    controller.publish_snapshot()?;
                }
            }
        }
    }

    if controller.discovery_requested {
        let _ = controller.set_discovery(false);
    }
    controller.broker.cancel_current();
    Ok(())
}

impl ControllerState<'_> {
    fn handle_command(&mut self, command: BluetoothCommand) {
        let result = match command {
            BluetoothCommand::SetPowered(powered) => self.set_powered(powered),
            BluetoothCommand::SetDiscovery(enabled) => self.set_discovery(enabled),
            BluetoothCommand::Connect(address) => {
                self.start_operation(address, BluetoothDeviceOperation::Connecting);
                Ok(())
            }
            BluetoothCommand::Disconnect(address) => {
                self.start_operation(address, BluetoothDeviceOperation::Disconnecting);
                Ok(())
            }
            BluetoothCommand::Pair(address) => {
                self.start_operation(address, BluetoothDeviceOperation::Pairing);
                Ok(())
            }
            BluetoothCommand::Forget(address) => {
                self.start_operation(address, BluetoothDeviceOperation::Forgetting);
                Ok(())
            }
            BluetoothCommand::RespondPairing {
                prompt_id,
                response,
            } => self.broker.respond(prompt_id, response),
            BluetoothCommand::CancelPairing(address) => {
                self.broker.reject_address(&address);
                self.cancel_pairing(&address)
            }
        };
        if let Err(error) = result {
            self.top_error = Some(error.to_string());
        } else {
            self.top_error = None;
        }
        let _ = self.publish_snapshot();
    }

    fn handle_internal_event(&mut self, event: InternalEvent) {
        match event {
            InternalEvent::PromptOpened(mut prompt) => {
                prompt.device_name = self
                    .device_name(&prompt.address)
                    .unwrap_or_else(|| prompt.address.clone());
                self.pairing_prompt = Some(prompt);
            }
            InternalEvent::PromptClosed(id) => {
                if self
                    .pairing_prompt
                    .as_ref()
                    .is_some_and(|prompt| prompt.id == id)
                {
                    self.pairing_prompt = None;
                }
            }
            InternalEvent::OperationFinished { address, result } => {
                self.operations.remove(&address);
                self.broker.finish_pairing(&address);
                match result {
                    Ok(()) => {
                        self.errors.remove(&address);
                    }
                    Err(error) => {
                        self.errors.insert(address, error);
                    }
                }
            }
        }
        let _ = self.publish_snapshot();
    }

    fn start_operation(&mut self, address: String, operation: BluetoothDeviceOperation) {
        if self.operations.contains_key(&address) {
            return;
        }
        let Some(path) = self.device_paths.get(&address).cloned() else {
            self.errors.insert(
                address,
                "Bluetooth device is no longer available".to_string(),
            );
            return;
        };
        self.errors.remove(&address);
        self.operations.insert(address.clone(), operation);
        if operation == BluetoothDeviceOperation::Pairing {
            self.broker.begin_pairing(&address);
        }
        let connection = self.connection.clone();
        let adapter_path = self.adapter_path.clone();
        let events = self.internal_sender.clone();
        thread::spawn(move || {
            let result =
                run_device_operation(&connection, adapter_path.as_deref(), &path, operation)
                    .map_err(|error| error.to_string());
            let _ = events.send(InternalEvent::OperationFinished { address, result });
        });
    }

    fn set_powered(&mut self, powered: bool) -> Result<()> {
        let path = self
            .adapter_path
            .as_deref()
            .context("Bluetooth adapter is unavailable")?;
        let proxy = bluez_proxy(&self.connection, path, BLUEZ_ADAPTER_INTERFACE)?;
        proxy
            .set_property("Powered", powered)
            .context("failed to change Bluetooth power")?;
        if !powered {
            self.discovery_requested = false;
            self.nearby_seen.clear();
        }
        Ok(())
    }

    fn set_discovery(&mut self, enabled: bool) -> Result<()> {
        if self.discovery_requested == enabled {
            return Ok(());
        }
        let path = self
            .adapter_path
            .as_deref()
            .context("Bluetooth adapter is unavailable")?;
        let proxy = bluez_proxy(&self.connection, path, BLUEZ_ADAPTER_INTERFACE)?;
        if enabled {
            self.nearby_seen.clear();
            proxy
                .call::<_, _, ()>("StartDiscovery", &())
                .context("failed to start Bluetooth discovery")?;
        } else {
            proxy
                .call::<_, _, ()>("StopDiscovery", &())
                .context("failed to stop Bluetooth discovery")?;
            self.nearby_seen.clear();
        }
        self.discovery_requested = enabled;
        Ok(())
    }

    fn cancel_pairing(&self, address: &str) -> Result<()> {
        let path = self
            .device_paths
            .get(address)
            .context("Bluetooth device is no longer available")?;
        bluez_proxy(&self.connection, path, BLUEZ_DEVICE_INTERFACE)?
            .call::<_, _, ()>("CancelPairing", &())
            .context("failed to cancel Bluetooth pairing")
    }

    fn note_signal(&mut self, message: &zbus::Message) {
        if !self.discovery_requested {
            return;
        }
        let header = message.header();
        let Some(path) = header.path() else {
            return;
        };
        let address = address_from_path(path.as_str());
        if address != path.as_str() {
            self.nearby_seen.insert(address, Instant::now());
        }
    }

    fn device_name(&self, address: &str) -> Option<String> {
        read_managed_objects(&self.connection)
            .ok()?
            .devices
            .into_iter()
            .find(|device| device.address == address)
            .map(|device| device.name)
    }

    fn publish_snapshot(&mut self) -> Result<()> {
        let parsed = read_managed_objects(&self.connection)?;
        self.adapter_path = parsed.adapter_path.clone();
        self.device_paths = parsed
            .devices
            .iter()
            .map(|device| (device.address.clone(), device.path.clone()))
            .collect();
        let state = build_bluetooth_state(
            parsed,
            &self.nearby_seen,
            &self.operations,
            &self.errors,
            self.pairing_prompt.clone(),
            self.top_error.clone(),
        );
        self.state_sender
            .send(StateUpdate::System(SystemUpdate::Bluetooth(state)))
            .map_err(|_| anyhow!("Bluetooth state receiver was dropped"))?;
        let _ = self.state_sender.send(StateUpdate::Health {
            source: SourceId::Bluetooth,
            health: SourceHealth::Healthy,
        });
        Ok(())
    }
}

fn run_device_operation(
    connection: &Connection,
    adapter_path: Option<&str>,
    device_path: &str,
    operation: BluetoothDeviceOperation,
) -> Result<()> {
    let device = bluez_proxy(connection, device_path, BLUEZ_DEVICE_INTERFACE)?;
    match operation {
        BluetoothDeviceOperation::Connecting => device
            .call::<_, _, ()>("Connect", &())
            .context("failed to connect Bluetooth device"),
        BluetoothDeviceOperation::Disconnecting => device
            .call::<_, _, ()>("Disconnect", &())
            .context("failed to disconnect Bluetooth device"),
        BluetoothDeviceOperation::Pairing => {
            device
                .call::<_, _, ()>("Pair", &())
                .context("failed to pair Bluetooth device")?;
            device
                .set_property("Trusted", true)
                .context("device paired, but could not be marked trusted")?;
            let connected = device.get_property::<bool>("Connected").unwrap_or(false);
            if !connected {
                device
                    .call::<_, _, ()>("Connect", &())
                    .context("device paired, but could not be connected")?;
            }
            Ok(())
        }
        BluetoothDeviceOperation::Forgetting => {
            let adapter_path = adapter_path.context("Bluetooth adapter is unavailable")?;
            let adapter = bluez_proxy(connection, adapter_path, BLUEZ_ADAPTER_INTERFACE)?;
            let path = OwnedObjectPath::try_from(device_path.to_string())?;
            adapter
                .call::<_, _, ()>("RemoveDevice", &(path,))
                .context("failed to forget Bluetooth device")
        }
    }
}

fn bluez_proxy<'a>(
    connection: &'a Connection,
    path: &'a str,
    interface: &'a str,
) -> Result<Proxy<'a>> {
    Proxy::new(connection, BLUEZ_DESTINATION, path, interface).map_err(Into::into)
}

fn bluez_signal_stream(connection: &Connection) -> Result<MessageIterator> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(BLUEZ_DESTINATION)?
        .path_namespace(BLUEZ_PATH_NAMESPACE)?
        .build();
    MessageIterator::for_match_rule(rule, connection, Some(64))
        .context("failed to subscribe to BlueZ signals")
}

fn read_managed_objects(connection: &Connection) -> Result<ParsedBluetoothState> {
    let object_manager =
        ObjectManagerProxy::new(connection, BLUEZ_DESTINATION, BLUEZ_OBJECT_MANAGER_PATH)
            .context("failed to build BlueZ object manager proxy")?;
    let managed = object_manager
        .get_managed_objects()
        .context("failed to read BlueZ managed objects")?;
    parse_managed_objects(managed)
}

fn parse_managed_objects(managed: ManagedObjects) -> Result<ParsedBluetoothState> {
    let mut parsed = ParsedBluetoothState::default();
    let mut batteries = HashMap::new();

    for (path, interfaces) in &managed {
        if let Some(adapter) = interfaces.get(BLUEZ_ADAPTER_INTERFACE) {
            parsed.available = true;
            parsed.powered = bool_property(adapter, "Powered").unwrap_or(false);
            parsed.discovering = bool_property(adapter, "Discovering").unwrap_or(false);
            parsed.adapter_path = Some(path.to_string());
        }
        if let Some(battery) = interfaces.get(BLUEZ_BATTERY_INTERFACE) {
            batteries.insert(path.to_string(), u8_property(battery, "Percentage"));
        }
    }

    for (path, interfaces) in managed {
        let Some(device) = interfaces.get(BLUEZ_DEVICE_INTERFACE) else {
            continue;
        };
        let address =
            string_property(device, "Address").unwrap_or_else(|| address_from_path(path.as_str()));
        let name = string_property(device, "Alias")
            .or_else(|| string_property(device, "Name"))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Unknown · {}", short_address(&address)));
        let bluez_icon = string_property(device, "Icon");
        let audio_capable = string_vec_property(device, "UUIDs")
            .into_iter()
            .flatten()
            .any(|uuid| is_audio_uuid(&uuid));
        parsed.devices.push(DeviceRecord {
            path: path.to_string(),
            address,
            name,
            icon_name: device_icon(bluez_icon.as_deref(), audio_capable).to_string(),
            paired: bool_property(device, "Paired").unwrap_or(false),
            trusted: bool_property(device, "Trusted").unwrap_or(false),
            connected: bool_property(device, "Connected").unwrap_or(false),
            audio_capable,
            battery_percent: batteries.get(path.as_str()).copied().flatten(),
            rssi: i16_property(device, "RSSI"),
        });
    }
    parsed
        .devices
        .sort_by(|left, right| left.name.cmp(&right.name));
    Ok(parsed)
}

fn build_bluetooth_state(
    parsed: ParsedBluetoothState,
    nearby_seen: &HashMap<String, Instant>,
    operations: &BTreeMap<String, BluetoothDeviceOperation>,
    errors: &BTreeMap<String, String>,
    pairing_prompt: Option<BluetoothPairingPrompt>,
    error: Option<String>,
) -> BluetoothState {
    if !parsed.available || !parsed.powered {
        return BluetoothState {
            available: parsed.available,
            powered: parsed.powered,
            discovering: parsed.discovering,
            pairing_prompt,
            error,
            ..BluetoothState::default()
        };
    }

    let mut devices = parsed
        .devices
        .into_iter()
        .filter(|device| {
            device.paired || device.connected || nearby_seen.contains_key(&device.address)
        })
        .map(|device| BluetoothDeviceState {
            operation: operations.get(&device.address).copied(),
            error: errors.get(&device.address).cloned(),
            address: device.address,
            name: device.name,
            icon_name: device.icon_name,
            paired: device.paired,
            trusted: device.trusted,
            connected: device.connected,
            audio_capable: device.audio_capable,
            battery_percent: device.battery_percent,
            rssi: device.rssi,
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then_with(|| right.paired.cmp(&left.paired))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let connected = devices
        .iter()
        .filter(|device| device.connected)
        .collect::<Vec<_>>();
    let connected_device = match connected.len() {
        0 => None,
        1 => Some(connected[0].name.clone()),
        count => Some(format!("{count} devices")),
    };
    let audio_device = connected
        .iter()
        .find(|device| device.audio_capable)
        .map(|device| device.name.clone());

    BluetoothState {
        available: true,
        powered: true,
        connected_device,
        audio_device,
        discovering: parsed.discovering,
        devices,
        pairing_prompt,
        error,
    }
}

fn address_from_path(path: &str) -> String {
    path.rsplit_once("/dev_")
        .map(|(_, address)| address.replace('_', ":"))
        .unwrap_or_else(|| path.to_string())
}

fn short_address(address: &str) -> String {
    address
        .split(':')
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(":")
}

fn string_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    <String as TryFrom<OwnedValue>>::try_from(properties.get(key)?.clone()).ok()
}

fn bool_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    <bool as TryFrom<OwnedValue>>::try_from(properties.get(key)?.clone()).ok()
}

fn u8_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    <u8 as TryFrom<OwnedValue>>::try_from(properties.get(key)?.clone()).ok()
}

fn i16_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<i16> {
    <i16 as TryFrom<OwnedValue>>::try_from(properties.get(key)?.clone()).ok()
}

fn string_vec_property(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<Vec<String>> {
    <Vec<String> as TryFrom<OwnedValue>>::try_from(properties.get(key)?.clone()).ok()
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

fn device_icon(icon: Option<&str>, audio_capable: bool) -> &'static str {
    match icon.unwrap_or_default() {
        "audio-card" | "audio-headset" | "audio-headphones" => "audio-headphones-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "input-mouse" | "input-tablet" => "input-mouse-symbolic",
        "input-gaming" => "input-gaming-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        _ if audio_capable => "audio-headphones-symbolic",
        _ => "bluetooth-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Instant;

    use crate::{BluetoothDeviceOperation, BluetoothPairingPromptKind, BluetoothPairingResponse};

    use super::{
        DeviceRecord, InternalEvent, PairingBroker, ParsedBluetoothState, address_from_path,
        build_bluetooth_state, device_icon, short_address,
    };

    #[test]
    fn device_paths_round_trip_to_addresses() {
        assert_eq!(
            address_from_path("/org/bluez/hci0/dev_88_C9_E8_25_7B_04"),
            "88:C9:E8:25:7B:04"
        );
        assert_eq!(short_address("88:C9:E8:25:7B:04"), "25:7B:04");
    }

    #[test]
    fn bluez_icons_map_to_installed_symbolic_icons() {
        assert_eq!(
            device_icon(Some("audio-headset"), false),
            "audio-headphones-symbolic"
        );
        assert_eq!(
            device_icon(Some("input-gaming"), false),
            "input-gaming-symbolic"
        );
        assert_eq!(device_icon(None, true), "audio-headphones-symbolic");
    }

    #[test]
    fn state_keeps_saved_devices_and_only_current_scan_results() {
        let parsed = ParsedBluetoothState {
            available: true,
            powered: true,
            discovering: true,
            adapter_path: Some("/org/bluez/hci0".to_string()),
            devices: vec![
                device("AA:00:00:00:00:01", "Headphones", true, true, true),
                device("AA:00:00:00:00:02", "Saved pad", true, false, false),
                device("AA:00:00:00:00:03", "Nearby keyboard", false, false, false),
                device(
                    "AA:00:00:00:00:04",
                    "Stale cached device",
                    false,
                    false,
                    false,
                ),
            ],
        };
        let nearby = HashMap::from([("AA:00:00:00:00:03".to_string(), Instant::now())]);
        let operations = BTreeMap::from([(
            "AA:00:00:00:00:02".to_string(),
            BluetoothDeviceOperation::Connecting,
        )]);
        let errors = BTreeMap::new();

        let state = build_bluetooth_state(parsed, &nearby, &operations, &errors, None, None);

        assert_eq!(state.connected_device.as_deref(), Some("Headphones"));
        assert_eq!(state.audio_device.as_deref(), Some("Headphones"));
        assert_eq!(state.devices.len(), 3);
        assert!(
            state
                .devices
                .iter()
                .any(|device| device.name == "Nearby keyboard")
        );
        assert!(
            !state
                .devices
                .iter()
                .any(|device| device.name == "Stale cached device")
        );
        assert_eq!(
            state
                .devices
                .iter()
                .find(|device| device.name == "Saved pad")
                .and_then(|device| device.operation),
            Some(BluetoothDeviceOperation::Connecting)
        );
    }

    #[test]
    fn pairing_broker_round_trips_inline_confirmation() {
        let (events, receiver) = mpsc::channel();
        let broker = Arc::new(PairingBroker::new(events));
        broker.begin_pairing("AA:BB:CC:DD:EE:FF");
        let broker_for_prompt = broker.clone();
        let waiter = thread::spawn(move || {
            broker_for_prompt.ask(
                "AA:BB:CC:DD:EE:FF".to_string(),
                BluetoothPairingPromptKind::ConfirmPasskey { passkey: 42 },
            )
        });
        let prompt_id = match receiver.recv().unwrap() {
            InternalEvent::PromptOpened(prompt) => prompt.id,
            event => panic!("unexpected event: {event:?}"),
        };
        broker
            .respond(prompt_id, BluetoothPairingResponse::Accept)
            .unwrap();
        assert_eq!(
            waiter.join().unwrap().unwrap(),
            BluetoothPairingResponse::Accept
        );
    }

    fn device(
        address: &str,
        name: &str,
        paired: bool,
        connected: bool,
        audio_capable: bool,
    ) -> DeviceRecord {
        DeviceRecord {
            path: format!("/org/bluez/hci0/dev_{}", address.replace(':', "_")),
            address: address.to_string(),
            name: name.to_string(),
            icon_name: device_icon(None, audio_capable).to_string(),
            paired,
            trusted: paired,
            connected,
            audio_capable,
            battery_percent: connected.then_some(72),
            rssi: None,
        }
    }
}
