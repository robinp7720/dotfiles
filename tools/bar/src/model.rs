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
    pub changed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemState {
    pub keyboard_layout: Option<String>,
    pub resources: ResourceState,
    pub network: NetworkState,
    pub bluetooth: BluetoothState,
    pub power: PowerState,
    pub clock: ClockState,
    pub media: Option<MediaState>,
    pub calendar: Option<CalendarEvent>,
    pub timers: Vec<TimerState>,
    pub source_health: BTreeMap<SourceId, SourceHealth>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemUpdate {
    KeyboardLayout(Option<String>),
    Resources(ResourceState),
    Network(NetworkState),
    Bluetooth(BluetoothState),
    Power(PowerState),
    Clock(ClockState),
    Media(Option<MediaState>),
    Calendar(Option<CalendarEvent>),
    Timers(Vec<TimerState>),
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BluetoothState {
    pub powered: bool,
    pub connected_device: Option<String>,
    pub audio_device: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PowerProfile {
    Performance,
    #[default]
    Balanced,
    PowerSaver,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerState {
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
    OpenWindowSearch,
    OpenContextQuery {
        query: String,
    },
    ControlMedia(MediaControlAction),
    CyclePowerProfile {
        direction: Direction,
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
