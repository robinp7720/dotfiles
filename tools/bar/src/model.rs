use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarSnapshot {
    pub outputs: BTreeMap<String, OutputState>,
    pub focused_output: Option<String>,
    pub system: SystemState,
    pub activities: ActivityState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRole {
    Primary,
    Reduced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceHealth {
    Healthy,
    Stale { since_epoch: i64 },
    Disconnected { message: String },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SerializableSourceHealth {
    Healthy,
    Stale { since_epoch: i64 },
    Disconnected { message: String },
}

impl Serialize for SourceHealth {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = match self {
            SourceHealth::Healthy => SerializableSourceHealth::Healthy,
            SourceHealth::Stale { since_epoch } => SerializableSourceHealth::Stale {
                since_epoch: *since_epoch,
            },
            SourceHealth::Disconnected { message } => SerializableSourceHealth::Disconnected {
                message: message.clone(),
            },
        };

        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SourceHealth {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = SerializableSourceHealth::deserialize(deserializer)?;
        Ok(match value {
            SerializableSourceHealth::Healthy => SourceHealth::Healthy,
            SerializableSourceHealth::Stale { since_epoch } => SourceHealth::Stale { since_epoch },
            SerializableSourceHealth::Disconnected { message } => {
                SourceHealth::Disconnected { message }
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StateUpdate {
    Outputs(Vec<OutputState>),
    FocusedOutput(Option<String>),
    System(SystemUpdate),
    Activity(ActivityUpdate),
    Health {
        source: SourceId,
        health: SourceHealth,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputState {
    pub name: String,
    pub workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub windows: Vec<WindowState>,
    pub focused_window: Option<WindowState>,
    pub urgent: bool,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub id: String,
    pub label: String,
    pub output: String,
    pub active: bool,
    pub urgent: bool,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub id: String,
    pub app_id: Option<String>,
    pub title: String,
    pub urgent: bool,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemState {
    #[serde(default)]
    pub keyboard_layout: KeyboardLayoutState,
    pub resources: ResourceState,
    pub network: NetworkState,
    pub bluetooth: BluetoothState,
    pub audio: AudioState,
    #[serde(default)]
    pub brightness: BrightnessState,
    pub power: PowerState,
    pub clock: ClockState,
    pub media: Option<MediaState>,
    pub calendar: Option<CalendarEvent>,
    pub timers: Vec<TimerState>,
    pub source_health: BTreeMap<SourceId, SourceHealth>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemUpdate {
    KeyboardLayout(KeyboardLayoutState),
    Resources(ResourceState),
    Network(NetworkState),
    Bluetooth(BluetoothState),
    Audio(AudioState),
    Brightness(BrightnessState),
    Power(PowerState),
    Clock(ClockState),
    Media(Option<MediaState>),
    Calendar(Option<CalendarEvent>),
    Timers(Vec<TimerState>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayoutState {
    pub current_index: Option<u8>,
    pub current_name: Option<String>,
    #[serde(default)]
    pub layouts: Vec<KeyboardLayoutOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayoutOption {
    pub index: u8,
    pub name: String,
    pub layout: Option<String>,
    pub variant: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceState {
    pub cpu_percent: Option<u8>,
    pub memory_percent: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectivityState {
    #[default]
    Unknown,
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkState {
    pub connectivity: ConnectivityState,
    pub icon_hint: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub wifi_available: bool,
    #[serde(default)]
    pub ethernet_available: bool,
    #[serde(default)]
    pub wifi_enabled: Option<bool>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub download_bytes_per_second: Option<u64>,
    #[serde(default)]
    pub upload_bytes_per_second: Option<u64>,
    #[serde(default)]
    pub download_history: Vec<u64>,
    #[serde(default)]
    pub upload_history: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothState {
    #[serde(default)]
    pub available: bool,
    pub powered: bool,
    pub connected_device: Option<String>,
    pub audio_device: Option<String>,
    #[serde(default)]
    pub discovering: bool,
    #[serde(default)]
    pub devices: Vec<BluetoothDeviceState>,
    #[serde(default)]
    pub pairing_prompt: Option<BluetoothPairingPrompt>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothDeviceState {
    pub address: String,
    pub name: String,
    pub icon_name: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub audio_capable: bool,
    pub battery_percent: Option<u8>,
    pub rssi: Option<i16>,
    pub operation: Option<BluetoothDeviceOperation>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothDeviceOperation {
    Connecting,
    Disconnecting,
    Pairing,
    Forgetting,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothPairingPrompt {
    pub id: u64,
    pub address: String,
    pub device_name: String,
    pub kind: BluetoothPairingPromptKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothPairingPromptKind {
    ConfirmPasskey { passkey: u32 },
    EnterPinCode,
    EnterPasskey,
    DisplayPinCode { pin_code: String },
    DisplayPasskey { passkey: u32, entered: u16 },
    Authorize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothPairingResponse {
    Accept,
    PinCode(String),
    Passkey(u32),
    Reject,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioState {
    pub volume_percent: Option<u8>,
    pub muted: bool,
    #[serde(default)]
    pub outputs: Vec<AudioOutputState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioOutputState {
    pub name: String,
    pub description: String,
    pub alias: Option<String>,
    pub port_description: Option<String>,
    pub port_type: Option<String>,
    pub bus: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessState {
    pub device: Option<String>,
    pub percent: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PowerProfile {
    Performance,
    #[default]
    Balanced,
    PowerSaver,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopContext {
    #[default]
    Overview,
    Keyboard,
    Resources,
    Network,
    Bluetooth,
    Audio,
    Power,
    Clock,
}

impl DesktopContext {
    pub const ALL: [Self; 8] = [
        Self::Overview,
        Self::Keyboard,
        Self::Resources,
        Self::Network,
        Self::Bluetooth,
        Self::Audio,
        Self::Power,
        Self::Clock,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Keyboard => "keyboard",
            Self::Resources => "resources",
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Clock => "clock",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextHealth {
    #[default]
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextAction {
    SelectKeyboardLayout {
        index: u8,
    },
    SetWifiEnabled {
        enabled: bool,
    },
    SetBluetoothPowered {
        powered: bool,
    },
    ConnectBluetoothDevice {
        address: String,
    },
    DisconnectBluetoothDevice {
        address: String,
    },
    SetVolumePercent {
        percent: u8,
    },
    ToggleMute,
    SetAudioOutput {
        sink_name: String,
    },
    ControlMedia {
        player: String,
        action: MediaControlAction,
    },
    SetBrightnessPercent {
        device: String,
        percent: u8,
    },
    SetPowerProfile {
        profile: PowerProfile,
    },
    PauseTimer {
        id: String,
    },
    ResumeTimer {
        id: String,
    },
    CancelTimer {
        id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextActionSpec {
    pub title: String,
    pub subtitle: String,
    pub icon_name: String,
    pub accessory: Option<String>,
    pub action: ContextAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub context: DesktopContext,
    pub title: String,
    pub icon_name: String,
    pub summary: String,
    pub detail: String,
    pub health: ContextHealth,
    pub actions: Vec<ContextActionSpec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerState {
    #[serde(default)]
    pub battery_present: bool,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub profile: PowerProfile,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub epoch_seconds: i64,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaState {
    pub player: String,
    pub status: PlaybackStatus,
    pub title: Option<String>,
    pub artist: Option<String>,
    #[serde(default)]
    pub art_url: Option<String>,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub location: Option<String>,
    pub start_epoch: i64,
    pub end_epoch: Option<i64>,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerState {
    pub id: String,
    pub label: String,
    pub remaining_seconds: u64,
    pub target_epoch: Option<i64>,
    pub completed: bool,
    #[serde(default)]
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityState {
    pub items: BTreeMap<String, CommandActivity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActivityStatus {
    #[default]
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandActivity {
    pub id: String,
    pub label: String,
    pub cwd: PathBuf,
    pub status: ActivityStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityUpdate {
    Started(CommandActivity),
    Finished {
        id: String,
        finished_at: i64,
        exit_code: i32,
    },
    Snapshot(Vec<CommandActivity>),
    Removed {
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceId {
    Compositor,
    Power,
    Resources,
    Network,
    Bluetooth,
    Audio,
    Brightness,
    Media,
    Calendar,
    Timers,
    Activity,
    #[default]
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaControlAction {
    Previous,
    Next,
    PlayPause,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionIntent {
    SwitchWorkspace {
        output: String,
        workspace: String,
    },
    CycleWorkspace {
        output: String,
        direction: Direction,
    },
    FocusWindow {
        output: String,
        window_id: String,
    },
    ToggleKeyboardLayout,
    SelectKeyboardLayout {
        index: u8,
    },
    OpenWindowSearch {
        output: String,
    },
    OpenContextQuery {
        context: DesktopContext,
        output: String,
    },
    ControlMedia {
        player: String,
        action: MediaControlAction,
    },
    SetVolumePercent {
        percent: u8,
    },
    ToggleMute,
    SetAudioOutput {
        sink_name: String,
    },
    SetWifiEnabled {
        enabled: bool,
    },
    SetBluetoothPowered {
        powered: bool,
    },
    SetBluetoothDiscovery {
        enabled: bool,
    },
    ConnectBluetoothDevice {
        address: String,
    },
    DisconnectBluetoothDevice {
        address: String,
    },
    PairBluetoothDevice {
        address: String,
    },
    ForgetBluetoothDevice {
        address: String,
    },
    RespondBluetoothPairing {
        prompt_id: u64,
        response: BluetoothPairingResponse,
    },
    CancelBluetoothPairing {
        address: String,
    },
    SetBrightnessPercent {
        device: String,
        percent: u8,
    },
    CyclePowerProfile {
        direction: Direction,
    },
    SetPowerProfile {
        profile: PowerProfile,
    },
    StartTimer {
        label: String,
        duration_seconds: u64,
    },
    PauseTimer {
        id: String,
    },
    ResumeTimer {
        id: String,
    },
    CancelTimer {
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{AudioState, BluetoothState, MediaState, NetworkState, PowerState};

    #[test]
    fn network_state_deserializes_without_new_wifi_radio_field() {
        let state: NetworkState = serde_json::from_str(
            r#"{"connectivity":"Connected","icon_hint":null,"label":"Ethernet"}"#,
        )
        .unwrap();

        assert_eq!(state.wifi_enabled, None);
        assert!(!state.wifi_available);
        assert!(!state.ethernet_available);
        assert_eq!(state.interface, None);
        assert!(state.download_history.is_empty());
        assert!(state.upload_history.is_empty());
    }

    #[test]
    fn hardware_capabilities_default_to_absent_for_older_payloads() {
        let bluetooth: BluetoothState = serde_json::from_str(
            r#"{"powered":false,"connected_device":null,"audio_device":null}"#,
        )
        .unwrap();
        let power: PowerState = serde_json::from_str(
            r#"{"battery_percent":null,"charging":false,"profile":"Balanced","changed_at":0}"#,
        )
        .unwrap();

        assert!(!bluetooth.available);
        assert!(!power.battery_present);
    }

    #[test]
    fn media_state_deserializes_without_artwork_uri() {
        let state: MediaState = serde_json::from_str(
            r#"{"player":"firefox","status":"Playing","title":"Track","artist":"Artist"}"#,
        )
        .unwrap();

        assert_eq!(state.art_url, None);
    }

    #[test]
    fn audio_state_deserializes_without_output_inventory() {
        let state: AudioState =
            serde_json::from_str(r#"{"volume_percent":42,"muted":false}"#).unwrap();

        assert!(state.outputs.is_empty());
    }
}
