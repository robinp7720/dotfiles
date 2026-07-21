use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use tracing::warn;

use crate::{
    ActionCompletion, ActionIntent, ActionRequest, ActionResult, AudioOutputState, BarSnapshot,
    BluetoothDeviceOperation, BluetoothDeviceState, BluetoothPairingPrompt,
    BluetoothPairingPromptKind, BluetoothPairingResponse, CalendarAgenda, CalendarAgendaEvent,
    CalendarMonthRequest, ConnectivityState, DesktopContext, Direction, KeyboardLayoutOption,
    KeyboardLayoutState, MediaControlAction, PlaybackStatus, PowerProfile, SourceHealth, SourceId,
};

use super::artwork::{
    ArtworkRequest, prefer_artwork_candidate, result_is_current, spawn_artwork_loader,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlCenterFocus {
    Overview,
    Keyboard,
    Resources,
    Network,
    Bluetooth,
    Audio,
    Power,
    Clock,
}

impl ControlCenterFocus {
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

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Quick Settings",
            Self::Keyboard => "Keyboard",
            Self::Resources => "System Resources",
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Audio => "Audio",
            Self::Power => "Power",
            Self::Clock => "Time & Focus",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => "System overview",
            Self::Keyboard => "Input layout",
            Self::Resources => "CPU and memory",
            Self::Network => "Connectivity",
            Self::Bluetooth => "Devices and discovery",
            Self::Audio => "Volume and playback",
            Self::Power => "Performance and energy",
            Self::Clock => "Calendar and timers",
        }
    }

    pub fn luma_query(self) -> &'static str {
        match self {
            Self::Overview => "system",
            Self::Keyboard => "keyboard",
            Self::Resources => "system",
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Clock => "calendar",
        }
    }
}

impl From<DesktopContext> for ControlCenterFocus {
    fn from(context: DesktopContext) -> Self {
        match context {
            DesktopContext::Overview => Self::Overview,
            DesktopContext::Keyboard => Self::Keyboard,
            DesktopContext::Resources => Self::Resources,
            DesktopContext::Network => Self::Network,
            DesktopContext::Bluetooth => Self::Bluetooth,
            DesktopContext::Audio => Self::Audio,
            DesktopContext::Power => Self::Power,
            DesktopContext::Clock => Self::Clock,
        }
    }
}

impl From<ControlCenterFocus> for DesktopContext {
    fn from(focus: ControlCenterFocus) -> Self {
        match focus {
            ControlCenterFocus::Overview => Self::Overview,
            ControlCenterFocus::Keyboard => Self::Keyboard,
            ControlCenterFocus::Resources => Self::Resources,
            ControlCenterFocus::Network => Self::Network,
            ControlCenterFocus::Bluetooth => Self::Bluetooth,
            ControlCenterFocus::Audio => Self::Audio,
            ControlCenterFocus::Power => Self::Power,
            ControlCenterFocus::Clock => Self::Clock,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickControlSpec {
    pub icon_name: String,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
    pub available: bool,
    pub toggle_available: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkTrafficSpec {
    pub interface: Option<String>,
    pub download_bytes_per_second: Option<u64>,
    pub upload_bytes_per_second: Option<u64>,
    pub download_history: Vec<u64>,
    pub upload_history: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioControlSpec {
    pub percent: Option<u8>,
    pub muted: bool,
    pub detail: String,
    pub outputs: Vec<AudioOutputControlSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOutputControlSpec {
    pub name: String,
    pub label: String,
    pub detail: String,
    pub icon_name: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothDeviceControlSpec {
    pub address: String,
    pub name: String,
    pub icon_name: String,
    pub detail: String,
    pub paired: bool,
    pub connected: bool,
    pub operation: Option<BluetoothDeviceOperation>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BluetoothManagerSpec {
    pub available: bool,
    pub powered: bool,
    pub discovering: bool,
    pub connected_count: usize,
    pub devices: Vec<BluetoothDeviceControlSpec>,
    pub prompt: Option<BluetoothPairingPrompt>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardLayoutControlSpec {
    pub index: u8,
    pub title: String,
    pub detail: String,
    pub raw_name: String,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardControlSpec {
    pub summary: String,
    pub current: String,
    pub detail: String,
    pub layouts: Vec<KeyboardLayoutControlSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrightnessControlSpec {
    pub device: String,
    pub percent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaControlSpec {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub art_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimerControlSpec {
    pub id: String,
    pub label: String,
    pub remaining_seconds: u64,
    pub completed: bool,
    pub paused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarControlSpec {
    pub title: String,
    pub location: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlCenterSpec {
    pub network: QuickControlSpec,
    pub network_traffic: NetworkTrafficSpec,
    pub bluetooth: QuickControlSpec,
    pub bluetooth_manager: BluetoothManagerSpec,
    pub audio: AudioControlSpec,
    pub power: QuickControlSpec,
    pub brightness: Option<BrightnessControlSpec>,
    pub keyboard: KeyboardControlSpec,
    pub cpu_percent: Option<u8>,
    pub memory_percent: Option<u8>,
    pub battery_present: bool,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub clock: String,
    pub clock_epoch: i64,
    pub calendar: Option<CalendarControlSpec>,
    pub calendar_agenda: Option<CalendarAgenda>,
    pub calendar_agenda_error: Option<String>,
    pub timers: Vec<TimerControlSpec>,
    pub media: Option<MediaControlSpec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SliderDebounce {
    pending: Option<u8>,
}

impl SliderDebounce {
    pub fn schedule(&mut self, percent: u8) {
        self.pending = Some(percent.min(100));
    }

    pub fn take(&mut self) -> Option<u8> {
        self.pending.take()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlCenterErrors {
    messages: BTreeMap<ControlCenterFocus, String>,
}

impl ControlCenterErrors {
    pub fn record_failure(&mut self, focus: ControlCenterFocus, detail: &str) {
        self.messages.insert(focus, detail.to_string());
    }

    pub fn retry(&mut self, focus: ControlCenterFocus) {
        self.messages.remove(&focus);
    }

    pub fn close(&mut self) {
        self.messages.clear();
    }

    pub fn message(&self, focus: ControlCenterFocus) -> Option<&str> {
        self.messages.get(&focus).map(String::as_str)
    }
}

pub fn control_center_origin(focus: ControlCenterFocus, action: &str) -> String {
    format!("control-center:{}:{action}", focus.as_str())
}

pub fn focus_from_origin(origin: &str) -> Option<ControlCenterFocus> {
    let section = origin.strip_prefix("control-center:")?.split(':').next()?;
    ControlCenterFocus::ALL
        .into_iter()
        .find(|focus| focus.as_str() == section)
}

pub fn build_control_center_spec(snapshot: &BarSnapshot) -> ControlCenterSpec {
    let system = &snapshot.system;
    let network = network_control_spec(&system.network);
    let bluetooth_detail = system
        .bluetooth
        .connected_device
        .clone()
        .unwrap_or_else(|| {
            if system.bluetooth.powered {
                "On"
            } else {
                "Off"
            }
            .to_string()
        });
    let mut audio_outputs = system
        .audio
        .outputs
        .iter()
        .map(audio_output_control_spec)
        .collect::<Vec<_>>();
    audio_outputs.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    let audio_detail = audio_outputs
        .iter()
        .find(|output| output.selected)
        .map(|output| output.label.clone())
        .unwrap_or_else(|| "No audio output".to_string());
    let media = system.media.as_ref().map(|media| MediaControlSpec {
        player: media.player.clone(),
        title: media.title.clone().unwrap_or_else(|| media.player.clone()),
        artist: media.artist.clone().unwrap_or_else(|| media.player.clone()),
        playing: media.status == PlaybackStatus::Playing,
        art_url: media.art_url.clone(),
    });

    ControlCenterSpec {
        network,
        network_traffic: NetworkTrafficSpec {
            interface: system.network.interface.clone(),
            download_bytes_per_second: system.network.download_bytes_per_second,
            upload_bytes_per_second: system.network.upload_bytes_per_second,
            download_history: system.network.download_history.clone(),
            upload_history: system.network.upload_history.clone(),
        },
        bluetooth: QuickControlSpec {
            icon_name: "bluetooth-symbolic".to_string(),
            label: "Bluetooth".to_string(),
            detail: bluetooth_detail,
            enabled: system.bluetooth.powered,
            available: system.bluetooth.available,
            toggle_available: system.bluetooth.available,
        },
        bluetooth_manager: BluetoothManagerSpec {
            available: system.bluetooth.available,
            powered: system.bluetooth.powered,
            discovering: system.bluetooth.discovering,
            connected_count: system
                .bluetooth
                .devices
                .iter()
                .filter(|device| device.connected)
                .count(),
            devices: system
                .bluetooth
                .devices
                .iter()
                .map(|device| BluetoothDeviceControlSpec {
                    address: device.address.clone(),
                    name: device.name.clone(),
                    icon_name: device.icon_name.clone(),
                    detail: bluetooth_device_detail(device),
                    paired: device.paired,
                    connected: device.connected,
                    operation: device.operation,
                    error: device.error.clone(),
                })
                .collect(),
            prompt: system.bluetooth.pairing_prompt.clone(),
            error: system.bluetooth.error.clone(),
        },
        audio: AudioControlSpec {
            percent: system.audio.volume_percent.map(|value| value.min(100)),
            muted: system.audio.muted,
            detail: audio_detail,
            outputs: audio_outputs,
        },
        power: QuickControlSpec {
            icon_name: power_profile_icon(&system.power.profile).to_string(),
            label: "Power".to_string(),
            detail: power_profile_label(&system.power.profile).to_string(),
            enabled: system.power.profile != PowerProfile::Balanced,
            available: true,
            toggle_available: false,
        },
        brightness: system
            .brightness
            .device
            .as_ref()
            .zip(system.brightness.percent)
            .map(|(device, percent)| BrightnessControlSpec {
                device: device.clone(),
                percent: percent.min(100),
            }),
        keyboard: keyboard_control_spec(&system.keyboard_layout),
        cpu_percent: system.resources.cpu_percent,
        memory_percent: system.resources.memory_percent,
        battery_present: system.power.battery_present,
        battery_percent: system.power.battery_percent,
        charging: system.power.charging,
        clock: system.clock.label.clone(),
        clock_epoch: system.clock.epoch_seconds,
        calendar: system.calendar.as_ref().map(|event| CalendarControlSpec {
            title: event.title.clone(),
            location: event.location.clone(),
        }),
        calendar_agenda: system.calendar_agenda.clone(),
        calendar_agenda_error: match system.source_health.get(&SourceId::CalendarAgenda) {
            Some(SourceHealth::Disconnected { message }) => Some(message.clone()),
            Some(SourceHealth::Stale { .. }) => Some("Calendar agenda is stale".to_string()),
            _ => None,
        },
        timers: system
            .timers
            .iter()
            .map(|timer| TimerControlSpec {
                id: timer.id.clone(),
                label: timer.label.clone(),
                remaining_seconds: timer.remaining_seconds,
                completed: timer.completed,
                paused: !timer.completed && timer.target_epoch.is_none(),
            })
            .collect(),
        media,
    }
}

fn audio_output_control_spec(output: &AudioOutputState) -> AudioOutputControlSpec {
    let alias = output
        .alias
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let port = output
        .port_description
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let label = alias.or(port).unwrap_or(&output.description).to_string();
    let detail = if output.description != label {
        output.description.clone()
    } else if let Some(port) = port.filter(|port| *port != label) {
        port.to_string()
    } else {
        match output.bus.as_deref() {
            Some("bluetooth") => "Bluetooth audio".to_string(),
            Some("usb") => "USB audio".to_string(),
            Some("pci") => "Built-in audio".to_string(),
            _ => "Audio output".to_string(),
        }
    };
    let kind = output
        .port_type
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let name = output.name.to_lowercase();
    let icon_name = if kind.contains("headphone")
        || kind.contains("headset")
        || output.bus.as_deref() == Some("bluetooth")
        || name.contains("bluez")
    {
        "audio-headphones-symbolic"
    } else if kind.contains("hdmi") || kind.contains("display") || name.contains("hdmi") {
        "video-display-symbolic"
    } else if kind.contains("speaker") || kind.contains("line") {
        "audio-speakers-symbolic"
    } else {
        "audio-card-symbolic"
    };
    AudioOutputControlSpec {
        name: output.name.clone(),
        label,
        detail,
        icon_name: icon_name.to_string(),
        selected: output.is_default,
    }
}

fn bluetooth_device_detail(device: &BluetoothDeviceState) -> String {
    let state = match device.operation {
        Some(BluetoothDeviceOperation::Connecting) => "Connecting…".to_string(),
        Some(BluetoothDeviceOperation::Disconnecting) => "Disconnecting…".to_string(),
        Some(BluetoothDeviceOperation::Pairing) => "Pairing…".to_string(),
        Some(BluetoothDeviceOperation::Forgetting) => "Forgetting…".to_string(),
        None if device.connected => "Connected".to_string(),
        None if device.paired => "Saved".to_string(),
        None => match device.rssi {
            Some(rssi) if rssi >= -55 => "Nearby · Strong signal".to_string(),
            Some(rssi) if rssi >= -72 => "Nearby · Good signal".to_string(),
            Some(_) => "Nearby · Weak signal".to_string(),
            None => "Nearby".to_string(),
        },
    };
    match device.battery_percent {
        Some(percent) if device.connected => format!("{state} · {percent}% battery"),
        _ => state,
    }
}

fn network_control_spec(network: &crate::NetworkState) -> QuickControlSpec {
    if network.wifi_available {
        let enabled = network.wifi_enabled.unwrap_or(false);
        let wired_connection = network
            .icon_hint
            .as_deref()
            .is_some_and(|icon| icon.contains("wired"));
        let detail = if !enabled {
            "Off".to_string()
        } else if wired_connection {
            "On · Ethernet active".to_string()
        } else {
            network
                .label
                .clone()
                .unwrap_or_else(|| "Not connected".to_string())
        };
        return QuickControlSpec {
            icon_name: network
                .icon_hint
                .clone()
                .filter(|icon| icon.contains("wireless"))
                .unwrap_or_else(|| "network-wireless-symbolic".to_string()),
            label: "Wi-Fi".to_string(),
            detail,
            enabled,
            available: true,
            toggle_available: true,
        };
    }

    if network.ethernet_available {
        let enabled = network.connectivity == ConnectivityState::Connected;
        let detail = if enabled {
            network
                .label
                .clone()
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| "Connected".to_string())
        } else {
            match network.connectivity {
                ConnectivityState::Connecting => "Connecting".to_string(),
                _ => "Disconnected".to_string(),
            }
        };
        return QuickControlSpec {
            icon_name: "network-wired-symbolic".to_string(),
            label: "Ethernet".to_string(),
            detail,
            enabled,
            available: true,
            toggle_available: false,
        };
    }

    QuickControlSpec {
        icon_name: "network-offline-symbolic".to_string(),
        label: "Network".to_string(),
        detail: "Unavailable".to_string(),
        enabled: false,
        available: false,
        toggle_available: false,
    }
}

fn keyboard_control_spec(state: &KeyboardLayoutState) -> KeyboardControlSpec {
    let layouts = state
        .layouts
        .iter()
        .map(|option| {
            let (title, detail, _) = keyboard_labels(option);
            KeyboardLayoutControlSpec {
                index: option.index,
                title,
                detail,
                raw_name: option.name.clone(),
                selected: state.current_index == Some(option.index),
            }
        })
        .collect::<Vec<_>>();
    let current = state
        .current_index
        .and_then(|index| state.layouts.iter().find(|option| option.index == index));
    let (summary, current_label, detail) = if let Some(option) = current {
        let (title, variant, compact) = keyboard_labels(option);
        (
            compact,
            format!("{title} — {variant}"),
            state
                .current_name
                .clone()
                .unwrap_or_else(|| option.name.clone()),
        )
    } else if let Some(name) = state.current_name.as_deref() {
        let fallback = KeyboardLayoutOption {
            name: name.to_string(),
            ..KeyboardLayoutOption::default()
        };
        let (title, variant, compact) = keyboard_labels(&fallback);
        (compact, format!("{title} — {variant}"), name.to_string())
    } else {
        (
            "--".to_string(),
            "Unavailable".to_string(),
            "No active layout".to_string(),
        )
    };

    KeyboardControlSpec {
        summary,
        current: current_label,
        detail,
        layouts,
    }
}

fn keyboard_labels(option: &KeyboardLayoutOption) -> (String, String, String) {
    let raw = option.name.to_ascii_lowercase();
    let layout = option.layout.as_deref().map(str::to_ascii_lowercase);
    let variant = option.variant.as_deref().map(str::to_ascii_lowercase);
    let title = match layout.as_deref() {
        Some("us") => "US".to_string(),
        Some("de") => "DE".to_string(),
        Some(value) if !value.is_empty() => value.to_ascii_uppercase(),
        _ if raw.contains("english") || raw == "us" => "US".to_string(),
        _ if raw.contains("german") || raw == "de" => "DE".to_string(),
        _ => option
            .name
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase(),
    };
    let detail = match variant.as_deref() {
        Some("dvorak") => "Dvorak".to_string(),
        Some("koy") => "KOY".to_string(),
        Some(value) if !value.is_empty() => value.to_string(),
        _ if raw.contains("dvorak") => "Dvorak".to_string(),
        _ if raw.contains("koy") => "KOY".to_string(),
        _ => "Standard".to_string(),
    };
    let suffix = match detail.as_str() {
        "Standard" => None,
        "Dvorak" => Some("DV"),
        "KOY" => Some("KOY"),
        value => Some(value),
    };
    let compact = suffix.map_or_else(|| title.clone(), |suffix| format!("{title}-{suffix}"));
    (title, detail, compact)
}

pub(super) fn keyboard_bar_labels(state: &KeyboardLayoutState) -> (String, String) {
    let spec = keyboard_control_spec(state);
    (spec.summary, format!("{} · {}", spec.current, spec.detail))
}

fn power_profile_label(profile: &PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "Performance",
        PowerProfile::Balanced => "Balanced",
        PowerProfile::PowerSaver => "Power saver",
    }
}

fn power_profile_icon(profile: &PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "power-profile-performance-symbolic",
        PowerProfile::Balanced => "power-profile-balanced-symbolic",
        PowerProfile::PowerSaver => "power-profile-power-saver-symbolic",
    }
}

#[derive(Clone)]
struct NavigationUi {
    page: Rc<Cell<ControlCenterFocus>>,
    stack: gtk::Stack,
    back_button: gtk::Button,
    title: gtk::Label,
    subtitle: gtk::Label,
    footer_label: gtk::Label,
    error_slot: gtk::Box,
    error_label: gtk::Label,
    errors: Rc<RefCell<ControlCenterErrors>>,
    pending: Rc<RefCell<BTreeMap<ControlCenterFocus, usize>>>,
    page_changed: PageChanged,
}

type PageChanged = Rc<RefCell<Option<Rc<dyn Fn(ControlCenterFocus)>>>>;

impl NavigationUi {
    fn navigate(&self, page: ControlCenterFocus, backwards: bool) {
        self.page.set(page);
        self.stack.set_transition_type(if backwards {
            gtk::StackTransitionType::SlideRight
        } else {
            gtk::StackTransitionType::SlideLeft
        });
        self.stack.set_visible_child_name(page.as_str());
        self.back_button
            .set_visible(page != ControlCenterFocus::Overview);
        self.title.set_label(page.title());
        self.subtitle.set_label(page.subtitle());
        self.footer_label
            .set_label(&format!("Open {} in Luma", page.title()));
        self.render_error();
        if let Some(callback) = self.page_changed.borrow().as_ref() {
            callback(page);
        }
    }

    fn render_error(&self) {
        if let Some(message) = self.errors.borrow().message(self.page.get()) {
            self.error_label.set_label(message);
            self.error_slot.set_visible(true);
            self.error_slot.remove_css_class("is-pending");
            self.error_slot.add_css_class("has-error");
        } else if self
            .pending
            .borrow()
            .get(&self.page.get())
            .is_some_and(|count| *count > 0)
        {
            self.error_label.set_label("Applying change…");
            self.error_slot.set_visible(true);
            self.error_slot.remove_css_class("has-error");
            self.error_slot.add_css_class("is-pending");
        } else {
            self.error_label.set_label("");
            self.error_slot.set_visible(false);
            self.error_slot.remove_css_class("has-error");
            self.error_slot.remove_css_class("is-pending");
        }
    }

    fn begin_action(&self, focus: ControlCenterFocus) {
        *self.pending.borrow_mut().entry(focus).or_default() += 1;
        self.render_error();
    }

    fn finish_action(&self, focus: ControlCenterFocus) {
        let mut pending = self.pending.borrow_mut();
        if let Some(count) = pending.get_mut(&focus) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                pending.remove(&focus);
            }
        }
        drop(pending);
        self.render_error();
    }
}

#[derive(Clone)]
struct ActionHandle {
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
    focus: ControlCenterFocus,
}

impl ActionHandle {
    fn send(&self, action: &str, intent: ActionIntent) {
        self.errors.borrow_mut().retry(self.focus);
        self.navigation.begin_action(self.focus);
        if self
            .sender
            .send(ActionRequest {
                origin: control_center_origin(self.focus, action),
                intent,
            })
            .is_err()
        {
            self.navigation.finish_action(self.focus);
        }
    }
}

struct ToggleTile {
    root: gtk::Box,
    icon: gtk::Image,
    title: gtk::Label,
    detail: gtk::Label,
    toggle: gtk::Switch,
}

struct ActionTile {
    root: gtk::Box,
    icon: gtk::Image,
    title: gtk::Label,
    detail: gtk::Label,
    action: gtk::Button,
}

struct TileBody {
    button: gtk::Button,
    icon: gtk::Image,
    title: gtk::Label,
    detail: gtk::Label,
}

struct ConnectivityPage {
    root: gtk::Box,
    icon: gtk::Image,
    eyebrow: gtk::Label,
    state: gtk::Label,
    detail: gtk::Label,
    toggle_row: gtk::Box,
    toggle_label: gtk::Label,
    toggle: gtk::Switch,
}

struct BluetoothPage {
    root: gtk::Box,
    icon: gtk::Image,
    state: gtk::Label,
    detail: gtk::Label,
    toggle: gtk::Switch,
    list: gtk::Box,
    previous: RefCell<Option<BluetoothManagerSpec>>,
}

struct NetworkPage {
    connectivity: ConnectivityPage,
    traffic_root: gtk::Box,
    interface: gtk::Label,
    download: TrafficGraph,
    upload: TrafficGraph,
}

struct TrafficGraph {
    root: gtk::Box,
    value: gtk::Label,
    area: gtk::DrawingArea,
    history: Rc<RefCell<Vec<u64>>>,
}

#[derive(Clone)]
struct MediaWidgets {
    root: gtk::Overlay,
    artwork: gtk::Picture,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
}

struct ArtworkController {
    request_tx: Sender<ArtworkRequest>,
    requested: Rc<RefCell<Option<String>>>,
    track: RefCell<Option<(String, String)>>,
    generation: Rc<Cell<u64>>,
    best_pixels: Rc<Cell<u64>>,
    pending: Rc<RefCell<Option<glib::SourceId>>>,
}

impl ArtworkController {
    fn new(window: &gtk::ApplicationWindow, widgets: &[MediaWidgets]) -> Self {
        let (request_tx, result_rx) = spawn_artwork_loader();
        let requested = Rc::new(RefCell::new(None::<String>));
        let requested_for_poll = requested.clone();
        let generation = Rc::new(Cell::new(0_u64));
        let generation_for_poll = generation.clone();
        let best_pixels = Rc::new(Cell::new(0_u64));
        let best_pixels_for_poll = best_pixels.clone();
        let widgets_for_poll = widgets.to_vec();
        let window = window.downgrade();

        glib::timeout_add_local(Duration::from_millis(100), move || {
            if window.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }

            while let Ok(result) = result_rx.try_recv() {
                if !result_is_current(generation_for_poll.get(), result.generation) {
                    continue;
                }
                let result_uri = result.uri;
                let bytes = match result.bytes {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        warn!("media artwork unavailable: {error}");
                        if requested_for_poll.borrow().as_deref() == Some(result_uri.as_str()) {
                            requested_for_poll.borrow_mut().take();
                        }
                        continue;
                    }
                };
                let bytes = glib::Bytes::from_owned(bytes);
                match gtk::gdk::Texture::from_bytes(&bytes) {
                    Ok(texture) => {
                        let candidate_pixels = u64::try_from(texture.width())
                            .unwrap_or_default()
                            .saturating_mul(u64::try_from(texture.height()).unwrap_or_default());
                        if !prefer_artwork_candidate(best_pixels_for_poll.get(), candidate_pixels) {
                            continue;
                        }
                        best_pixels_for_poll.set(candidate_pixels);
                        for widget in &widgets_for_poll {
                            widget.artwork.set_paintable(Some(&texture));
                            widget.root.remove_css_class("missing-art");
                            widget.root.add_css_class("has-art");
                        }
                    }
                    Err(error) => {
                        warn!("media artwork could not be decoded: {error}");
                        if requested_for_poll.borrow().as_deref() == Some(result_uri.as_str()) {
                            requested_for_poll.borrow_mut().take();
                        }
                    }
                }
            }

            glib::ControlFlow::Continue
        });

        Self {
            request_tx,
            requested,
            track: RefCell::new(None),
            generation,
            best_pixels,
            pending: Rc::new(RefCell::new(None)),
        }
    }

    fn show(&self, media: Option<&MediaControlSpec>, widgets: &[MediaWidgets]) {
        let next_track = media.map(|media| (media.title.clone(), media.artist.clone()));
        let track_changed = self.track.borrow().as_ref() != next_track.as_ref();
        if track_changed {
            *self.track.borrow_mut() = next_track;
            *self.requested.borrow_mut() = None;
            self.generation.set(self.generation.get().wrapping_add(1));
            self.best_pixels.set(0);
            if let Some(source) = self.pending.borrow_mut().take() {
                source.remove();
            }
            clear_artwork(widgets);
        }

        let uri = media.and_then(|media| media.art_url.as_deref());
        if !track_changed && uri.is_none() {
            return;
        }
        if self.requested.borrow().as_deref() == uri {
            return;
        }

        *self.requested.borrow_mut() = uri.map(str::to_string);
        if let Some(source) = self.pending.borrow_mut().take() {
            source.remove();
        }

        if let Some(uri) = uri {
            let uri = uri.to_string();
            let generation = self.generation.get();
            let request_tx = self.request_tx.clone();
            let requested = self.requested.clone();
            let pending = self.pending.clone();
            if self.best_pixels.get() == 0 {
                let _ = request_tx.send(ArtworkRequest { uri, generation });
                return;
            }
            *self.pending.borrow_mut() = Some(glib::timeout_add_local_once(
                Duration::from_millis(300),
                move || {
                    pending.borrow_mut().take();
                    if requested.borrow().as_deref() == Some(uri.as_str()) {
                        let _ = request_tx.send(ArtworkRequest { uri, generation });
                    }
                },
            ));
        } else if track_changed {
            clear_artwork(widgets);
        }
    }
}

fn clear_artwork(widgets: &[MediaWidgets]) {
    for widget in widgets {
        widget
            .artwork
            .set_paintable(Option::<&gtk::gdk::Paintable>::None);
        widget.root.remove_css_class("has-art");
        widget.root.add_css_class("missing-art");
    }
}

struct TimerRowWidgets {
    root: gtk::Box,
    title: gtk::Label,
    remaining: gtk::Label,
    primary: gtk::Button,
    cancel: gtk::Button,
    current: Rc<RefCell<TimerControlSpec>>,
}

type TimerWidgets = BTreeMap<String, TimerRowWidgets>;

#[derive(Clone)]
struct TimePage {
    root: gtk::Box,
    clock: gtk::Label,
    date: gtk::Label,
    view_stack: gtk::Stack,
    calendar_tab: gtk::ToggleButton,
    calendar: gtk::Calendar,
    agenda_list: gtk::Box,
    agenda_empty: gtk::Label,
    agenda_status: gtk::Label,
    rendered_agenda: Rc<RefCell<Option<AgendaRenderKey>>>,
    expanded_event: Rc<RefCell<Option<String>>>,
    agenda_details: Rc<RefCell<BTreeMap<String, gtk::Box>>>,
    latest_spec: Rc<RefCell<ControlCenterSpec>>,
    calendar_action: ActionHandle,
    calendar_sender: Option<Sender<CalendarMonthRequest>>,
    timer_list: gtk::Box,
    timer_empty: gtk::Label,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AgendaRenderKey {
    year: i32,
    month: u32,
    day: i32,
    events: Vec<CalendarAgendaEvent>,
    error: Option<String>,
}

const METRIC_SEGMENTS: usize = 10;
const NETWORK_GRAPH_SAMPLES: usize = 60;
const CONTROL_CENTER_ENTER_MS: u64 = 320;
const CONTROL_CENTER_EXIT_MS: u64 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlCenterMotionPhase {
    Hidden,
    Entering,
    Visible,
    Exiting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlCenterMotionEvent {
    Present,
    Dismiss,
    Entered,
    Exited,
}

impl ControlCenterMotionPhase {
    fn transition(self, event: ControlCenterMotionEvent) -> Self {
        match (self, event) {
            (Self::Hidden | Self::Exiting, ControlCenterMotionEvent::Present) => Self::Entering,
            (Self::Entering | Self::Visible, ControlCenterMotionEvent::Dismiss) => Self::Exiting,
            (Self::Entering, ControlCenterMotionEvent::Entered) => Self::Visible,
            (Self::Exiting, ControlCenterMotionEvent::Exited) => Self::Hidden,
            _ => self,
        }
    }

    fn is_presented(self) -> bool {
        matches!(self, Self::Entering | Self::Visible)
    }
}

struct ControlCenterMotion {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    root: glib::WeakRef<gtk::Box>,
    phase: Cell<ControlCenterMotionPhase>,
    generation: Cell<u64>,
}

impl ControlCenterMotion {
    fn new(window: &gtk::ApplicationWindow, root: &gtk::Box) -> Self {
        Self {
            window: window.downgrade(),
            root: root.downgrade(),
            phase: Cell::new(ControlCenterMotionPhase::Hidden),
            generation: Cell::new(0),
        }
    }

    fn present(self: &Rc<Self>) {
        let current = self.phase.get();
        let next = current.transition(ControlCenterMotionEvent::Present);
        if next == current {
            return;
        }

        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };

        let generation = self.advance_generation();
        root.set_sensitive(true);
        root.remove_css_class("control-center-exiting");
        root.remove_css_class("control-center-entering");

        if !animations_enabled() {
            self.phase.set(ControlCenterMotionPhase::Visible);
            window.present();
            return;
        }

        self.phase.set(next);
        root.add_css_class("control-center-entering");
        window.present();

        let motion = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(CONTROL_CENTER_ENTER_MS), move || {
            let Some(motion) = motion.upgrade() else {
                return;
            };
            if motion.generation.get() != generation {
                return;
            }
            if let Some(root) = motion.root.upgrade() {
                root.remove_css_class("control-center-entering");
            }
            motion.phase.set(
                motion
                    .phase
                    .get()
                    .transition(ControlCenterMotionEvent::Entered),
            );
        });
    }

    fn dismiss(self: &Rc<Self>) {
        let current = self.phase.get();
        let next = current.transition(ControlCenterMotionEvent::Dismiss);
        if next == current {
            return;
        }

        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };

        let generation = self.advance_generation();
        root.remove_css_class("control-center-entering");
        root.remove_css_class("control-center-exiting");
        root.set_sensitive(false);

        if !animations_enabled() {
            self.phase.set(ControlCenterMotionPhase::Hidden);
            root.set_sensitive(true);
            window.set_visible(false);
            return;
        }

        self.phase.set(next);
        root.add_css_class("control-center-exiting");

        let motion = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(CONTROL_CENTER_EXIT_MS), move || {
            let Some(motion) = motion.upgrade() else {
                return;
            };
            if motion.generation.get() != generation {
                return;
            }
            if let Some(root) = motion.root.upgrade() {
                root.remove_css_class("control-center-exiting");
                root.set_sensitive(true);
            }
            motion.phase.set(
                motion
                    .phase
                    .get()
                    .transition(ControlCenterMotionEvent::Exited),
            );
            if let Some(window) = motion.window.upgrade() {
                window.set_visible(false);
            }
        });
    }

    fn is_presented(&self) -> bool {
        self.phase.get().is_presented()
    }

    fn destroy(&self) {
        self.advance_generation();
        self.phase.set(ControlCenterMotionPhase::Hidden);
        if let Some(root) = self.root.upgrade() {
            root.remove_css_class("control-center-entering");
            root.remove_css_class("control-center-exiting");
            root.set_sensitive(true);
        }
        if let Some(window) = self.window.upgrade() {
            window.close();
        }
    }

    fn advance_generation(&self) -> u64 {
        let next = self.generation.get().wrapping_add(1);
        self.generation.set(next);
        next
    }
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

struct MetricGauge {
    root: gtk::Box,
    value: gtk::Label,
    segments: Vec<gtk::Box>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MetricLevel {
    Normal,
    Warning,
    Critical,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
struct MetricVisual {
    active_segments: usize,
    level: MetricLevel,
    label: String,
}

pub struct ControlCenterView {
    window: gtk::ApplicationWindow,
    motion: Rc<ControlCenterMotion>,
    current: Rc<RefCell<ControlCenterSpec>>,
    navigation: NavigationUi,
    network_tile: ToggleTile,
    bluetooth_tile: ToggleTile,
    audio_tile: ToggleTile,
    power_tile: ActionTile,
    quick_grid: gtk::Grid,
    quick_layout: Cell<Option<(bool, bool)>>,
    volume_scales: Vec<gtk::Scale>,
    volume_values: Vec<gtk::Label>,
    volume_buttons: Vec<gtk::Button>,
    audio_output_list: gtk::Box,
    audio_output_specs: RefCell<Vec<AudioOutputControlSpec>>,
    brightness_rows: Vec<gtk::Box>,
    brightness_scales: Vec<gtk::Scale>,
    brightness_values: Vec<gtk::Label>,
    brightness_device: Rc<RefCell<Option<String>>>,
    media_widgets: Vec<MediaWidgets>,
    artwork: ArtworkController,
    keyboard_summary: gtk::Label,
    resources_summary: gtk::Label,
    battery_summary_button: gtk::Button,
    battery_summary: gtk::Label,
    time_summary: gtk::Label,
    network_page: NetworkPage,
    bluetooth_page: BluetoothPage,
    audio_state: gtk::Label,
    audio_detail: gtk::Label,
    battery_hero: gtk::Box,
    battery_state: gtk::Label,
    battery_detail: gtk::Label,
    power_profile: gtk::Label,
    keyboard_state: gtk::Label,
    keyboard_detail: gtk::Label,
    keyboard_layout_list: gtk::Box,
    keyboard_layout_specs: RefCell<Vec<KeyboardLayoutControlSpec>>,
    cpu_gauge: MetricGauge,
    memory_gauge: MetricGauge,
    time_page: TimePage,
    timer_widgets: RefCell<TimerWidgets>,
    clock_label: gtk::Label,
    errors: Rc<RefCell<ControlCenterErrors>>,
    suppress_controls: Rc<Cell<bool>>,
    bluetooth_discovery_active: Rc<Cell<bool>>,
    timer_sender: Sender<ActionRequest>,
}

impl ControlCenterView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        application: &gtk::Application,
        monitor: &gtk::gdk::Monitor,
        output_name: &str,
        top_margin: i32,
        right_margin: i32,
        spec: &ControlCenterSpec,
        action_sender: Sender<ActionRequest>,
        calendar_sender: Option<Sender<CalendarMonthRequest>>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("cockpit-quick-settings")
            .build();
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_default_size(512, -1);
        window.add_css_class("control-center-window");
        window.init_layer_shell();
        window.set_namespace(Some("cockpit-control-center"));
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Top, top_margin);
        window.set_margin(Edge::Right, right_margin);
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.set_monitor(Some(monitor));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("control-center-root");
        root.set_size_request(480, -1);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        header.add_css_class("control-header");
        let back_button = icon_button("go-previous-symbolic", "Back to overview");
        back_button.add_css_class("control-back");
        let heading_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        heading_text.set_hexpand(true);
        let title = gtk::Label::new(Some(ControlCenterFocus::Overview.title()));
        title.add_css_class("control-center-title");
        title.set_xalign(0.0);
        let subtitle = gtk::Label::new(Some(ControlCenterFocus::Overview.subtitle()));
        subtitle.add_css_class("supporting-text");
        subtitle.set_xalign(0.0);
        heading_text.append(&title);
        heading_text.append(&subtitle);
        let clock = gtk::Label::new(Some(&spec.clock));
        clock.add_css_class("control-center-clock");
        let close_button = gtk::Button::with_label("×");
        close_button.set_tooltip_text(Some("Close quick settings"));
        close_button.set_has_frame(false);
        close_button.add_css_class("control-close");
        header.append(&back_button);
        header.append(&heading_text);
        header.append(&clock);
        header.append(&close_button);
        root.append(&header);

        let stack = gtk::Stack::new();
        stack.add_css_class("control-stack");
        stack.set_transition_duration(180);
        stack.set_hhomogeneous(true);
        stack.set_vhomogeneous(false);

        let errors = Rc::new(RefCell::new(ControlCenterErrors::default()));
        let error_slot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        error_slot.add_css_class("control-error-slot");
        error_slot.set_size_request(-1, 32);
        error_slot.set_visible(false);
        let error_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
        let error_label = gtk::Label::new(None);
        error_label.set_xalign(0.0);
        error_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        error_label.set_hexpand(true);
        error_slot.append(&error_icon);
        error_slot.append(&error_label);

        let footer = gtk::Button::new();
        footer.add_css_class("control-footer");
        footer.set_has_frame(false);
        let footer_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        footer_content.set_halign(gtk::Align::Fill);
        let footer_icon = gtk::Image::from_icon_name("system-search-symbolic");
        footer_icon.add_css_class("control-footer-icon");
        let footer_label = gtk::Label::new(Some("Open Quick Settings in Luma"));
        footer_label.set_xalign(0.0);
        footer_label.set_hexpand(true);
        let footer_arrow = gtk::Image::from_icon_name("go-next-symbolic");
        footer_arrow.add_css_class("control-footer-arrow");
        footer_content.append(&footer_icon);
        footer_content.append(&footer_label);
        footer_content.append(&footer_arrow);
        footer.set_child(Some(&footer_content));

        let navigation = NavigationUi {
            page: Rc::new(Cell::new(ControlCenterFocus::Overview)),
            stack: stack.clone(),
            back_button: back_button.clone(),
            title,
            subtitle,
            footer_label,
            error_slot: error_slot.clone(),
            error_label,
            errors: errors.clone(),
            pending: Rc::new(RefCell::new(BTreeMap::new())),
            page_changed: Rc::new(RefCell::new(None)),
        };

        let current = Rc::new(RefCell::new(spec.clone()));
        let suppress_controls = Rc::new(Cell::new(false));
        let bluetooth_discovery_active = Rc::new(Cell::new(false));
        let discovery_sender = action_sender.clone();
        let discovery_current = current.clone();
        let discovery_active = bluetooth_discovery_active.clone();
        let brightness_device = Rc::new(RefCell::new(
            spec.brightness.as_ref().map(|value| value.device.clone()),
        ));

        let overview = gtk::Box::new(gtk::Orientation::Vertical, 12);
        overview.add_css_class("control-page");
        let quick_grid = gtk::Grid::new();
        quick_grid.add_css_class("control-grid");
        quick_grid.set_row_spacing(8);
        quick_grid.set_column_spacing(8);
        quick_grid.set_column_homogeneous(true);

        let network_tile = toggle_tile(
            "network-wireless-symbolic",
            "Wi-Fi",
            ControlCenterFocus::Network,
            &navigation,
        );
        let bluetooth_tile = toggle_tile(
            "bluetooth-symbolic",
            "Bluetooth",
            ControlCenterFocus::Bluetooth,
            &navigation,
        );
        let audio_tile = toggle_tile(
            "audio-volume-high-symbolic",
            "Audio",
            ControlCenterFocus::Audio,
            &navigation,
        );
        let power_tile = action_tile(
            "power-profile-balanced-symbolic",
            "Power",
            ControlCenterFocus::Power,
            &navigation,
        );
        overview.append(&section_eyebrow("CONTROLS"));
        overview.append(&quick_grid);

        let sliders = gtk::Box::new(gtk::Orientation::Vertical, 6);
        sliders.add_css_class("control-slider-group");
        let (
            overview_volume_row,
            overview_volume_scale,
            overview_volume_value,
            overview_volume_button,
        ) = volume_slider_row();
        let (overview_brightness_row, overview_brightness_scale, overview_brightness_value) =
            slider_row("display-brightness-symbolic", "Brightness");
        sliders.append(&overview_volume_row);
        sliders.append(&overview_brightness_row);
        overview.append(&section_eyebrow("LEVELS"));
        overview.append(&sliders);

        let overview_media = media_card(
            action_sender.clone(),
            errors.clone(),
            navigation.clone(),
            current.clone(),
        );
        overview.append(&overview_media.root);

        let summaries = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        summaries.add_css_class("summary-grid");
        summaries.set_homogeneous(true);
        let (keyboard_summary_button, keyboard_summary) =
            summary_tile("input-keyboard-symbolic", "Keyboard");
        let (resources_summary_button, resources_summary) =
            summary_tile("utilities-system-monitor-symbolic", "CPU / RAM");
        let (battery_summary_button, battery_summary) =
            summary_tile("battery-good-symbolic", "Battery");
        let (time_summary_button, time_summary) =
            summary_tile("preferences-system-time-symbolic", "Focus");
        connect_navigation(
            &keyboard_summary_button,
            ControlCenterFocus::Keyboard,
            &navigation,
        );
        connect_navigation(
            &resources_summary_button,
            ControlCenterFocus::Resources,
            &navigation,
        );
        connect_navigation(
            &battery_summary_button,
            ControlCenterFocus::Power,
            &navigation,
        );
        connect_navigation(&time_summary_button, ControlCenterFocus::Clock, &navigation);
        summaries.append(&keyboard_summary_button);
        summaries.append(&resources_summary_button);
        summaries.append(&battery_summary_button);
        summaries.append(&time_summary_button);
        overview.append(&section_eyebrow("AT A GLANCE"));
        overview.append(&summaries);
        stack.add_named(&overview, Some(ControlCenterFocus::Overview.as_str()));

        let network_page = network_page();
        stack.add_named(
            &network_page.connectivity.root,
            Some(ControlCenterFocus::Network.as_str()),
        );

        let bluetooth_page = bluetooth_page();
        stack.add_named(
            &bluetooth_page.root,
            Some(ControlCenterFocus::Bluetooth.as_str()),
        );

        let audio_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        audio_page.add_css_class("control-page");
        let (audio_hero, audio_state, audio_detail) =
            detail_hero("audio-volume-high-symbolic", "Audio");
        audio_page.append(&audio_hero);
        let (detail_volume_row, detail_volume_scale, detail_volume_value, detail_volume_button) =
            volume_slider_row();
        audio_page.append(&detail_volume_row);
        audio_page.append(&section_eyebrow("OUTPUT"));
        let audio_output_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        audio_output_list.add_css_class("audio-output-list");
        audio_output_list.set_vexpand(false);
        audio_page.append(&audio_output_list);
        let detail_media = media_card(
            action_sender.clone(),
            errors.clone(),
            navigation.clone(),
            current.clone(),
        );
        audio_page.append(&detail_media.root);
        stack.add_named(&audio_page, Some(ControlCenterFocus::Audio.as_str()));

        let power_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        power_page.add_css_class("control-page");
        let (battery_hero, battery_state, battery_detail) =
            detail_hero("battery-good-symbolic", "Battery");
        power_page.append(&battery_hero);
        let profile_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        profile_row.add_css_class("detail-action-row");
        let profile_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        profile_text.set_hexpand(true);
        let profile_title = gtk::Label::new(Some("Power profile"));
        profile_title.set_xalign(0.0);
        let power_profile = gtk::Label::new(None);
        power_profile.add_css_class("supporting-text");
        power_profile.set_xalign(0.0);
        profile_text.append(&profile_title);
        profile_text.append(&power_profile);
        let profile_button = icon_button("view-refresh-symbolic", "Cycle power profile");
        profile_button.add_css_class("detail-row-action");
        profile_row.append(&profile_text);
        profile_row.append(&profile_button);
        power_page.append(&profile_row);
        let (detail_brightness_row, detail_brightness_scale, detail_brightness_value) =
            slider_row("display-brightness-symbolic", "Brightness");
        power_page.append(&detail_brightness_row);
        stack.add_named(&power_page, Some(ControlCenterFocus::Power.as_str()));

        let keyboard_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        keyboard_page.add_css_class("control-page");
        let (keyboard_hero, keyboard_state, keyboard_detail) =
            detail_hero("input-keyboard-symbolic", "Current layout");
        keyboard_page.append(&keyboard_hero);
        keyboard_page.append(&section_eyebrow("AVAILABLE LAYOUTS"));
        let keyboard_layout_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        keyboard_layout_list.add_css_class("keyboard-layout-list");
        keyboard_page.append(&keyboard_layout_list);
        stack.add_named(&keyboard_page, Some(ControlCenterFocus::Keyboard.as_str()));

        let resources_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        resources_page.add_css_class("control-page");
        let resources_intro = section_intro(
            "utilities-system-monitor-symbolic",
            "Live system pressure",
            "A quick read of processor and memory use.",
        );
        resources_page.append(&resources_intro);
        let cpu_gauge = metric_row("CPU");
        let memory_gauge = metric_row("Memory");
        resources_page.append(&cpu_gauge.root);
        resources_page.append(&memory_gauge.root);
        stack.add_named(
            &resources_page,
            Some(ControlCenterFocus::Resources.as_str()),
        );

        let time_page = build_time_page(
            spec,
            action_sender.clone(),
            errors.clone(),
            navigation.clone(),
            calendar_sender,
        );
        stack.add_named(&time_page.root, Some(ControlCenterFocus::Clock.as_str()));
        let time_page_for_navigation = time_page.clone();
        *navigation.page_changed.borrow_mut() = Some(Rc::new(move |page| {
            sync_bluetooth_discovery(
                &discovery_sender,
                &discovery_current,
                &discovery_active,
                page == ControlCenterFocus::Bluetooth,
            );
            if page == ControlCenterFocus::Clock {
                reset_time_page(&time_page_for_navigation);
            }
        }));

        root.append(&stack);
        root.append(&error_slot);
        root.append(&footer);
        window.set_child(Some(&root));

        let motion = Rc::new(ControlCenterMotion::new(&window, &root));

        let nav_for_back = navigation.clone();
        back_button.connect_clicked(move |_| {
            nav_for_back.navigate(ControlCenterFocus::Overview, true);
        });
        let motion_for_close_button = motion.clone();
        close_button.connect_clicked(move |_| {
            motion_for_close_button.dismiss();
        });
        let nav_for_footer = navigation.clone();
        let sender_for_footer = action_sender.clone();
        let motion_for_footer = motion.clone();
        let output_for_footer = output_name.to_string();
        footer.connect_clicked(move |_| {
            let focus = nav_for_footer.page.get();
            motion_for_footer.dismiss();
            let sender = sender_for_footer.clone();
            let output = output_for_footer.clone();
            glib::timeout_add_local_once(
                Duration::from_millis(CONTROL_CENTER_EXIT_MS),
                move || {
                    let _ = sender.send(ActionRequest {
                        origin: control_center_origin(focus, "open-luma"),
                        intent: ActionIntent::OpenContextQuery {
                            context: focus.into(),
                            output,
                        },
                    });
                },
            );
        });

        connect_toggle_controls(
            [&network_tile.toggle, &network_page.connectivity.toggle],
            suppress_controls.clone(),
            current.clone(),
            ActionHandle {
                sender: action_sender.clone(),
                errors: errors.clone(),
                navigation: navigation.clone(),
                focus: ControlCenterFocus::Network,
            },
            |spec, active| {
                (spec.network.toggle_available && spec.network.enabled != active)
                    .then_some(ActionIntent::SetWifiEnabled { enabled: active })
            },
        );
        connect_toggle_controls(
            [&bluetooth_tile.toggle, &bluetooth_page.toggle],
            suppress_controls.clone(),
            current.clone(),
            ActionHandle {
                sender: action_sender.clone(),
                errors: errors.clone(),
                navigation: navigation.clone(),
                focus: ControlCenterFocus::Bluetooth,
            },
            |spec, active| {
                (spec.bluetooth.toggle_available && spec.bluetooth.enabled != active)
                    .then_some(ActionIntent::SetBluetoothPowered { powered: active })
            },
        );
        let audio_action = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Audio,
        };
        connect_toggle_controls(
            [&audio_tile.toggle],
            suppress_controls.clone(),
            current.clone(),
            audio_action.clone(),
            |spec, active| (spec.audio.muted == active).then_some(ActionIntent::ToggleMute),
        );
        for button in [&overview_volume_button, &detail_volume_button] {
            let handle = audio_action.clone();
            button.connect_clicked(move |_| {
                handle.send("toggle-mute", ActionIntent::ToggleMute);
            });
        }

        let power_action = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Power,
        };
        for button in [&power_tile.action, &profile_button] {
            let handle = power_action.clone();
            button.connect_clicked(move |_| {
                handle.send(
                    "cycle-profile",
                    ActionIntent::CyclePowerProfile {
                        direction: Direction::Next,
                    },
                );
            });
        }

        let volume_handle = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Audio,
        };
        for scale in [&overview_volume_scale, &detail_volume_scale] {
            install_percent_debounce(
                scale,
                suppress_controls.clone(),
                volume_handle.clone(),
                "set-volume",
                |percent| ActionIntent::SetVolumePercent { percent },
            );
        }

        let brightness_handle = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Power,
        };
        for scale in [&overview_brightness_scale, &detail_brightness_scale] {
            let device = brightness_device.clone();
            install_percent_debounce(
                scale,
                suppress_controls.clone(),
                brightness_handle.clone(),
                "set-brightness",
                move |percent| ActionIntent::SetBrightnessPercent {
                    device: device.borrow().clone().unwrap_or_default(),
                    percent,
                },
            );
        }

        let errors_for_close = errors.clone();
        let nav_for_close = navigation.clone();
        let current_for_close = current.clone();
        let discovery_for_close = bluetooth_discovery_active.clone();
        let sender_for_close = action_sender.clone();
        window.connect_visible_notify(move |window| {
            if !window.is_visible() {
                errors_for_close.borrow_mut().close();
                nav_for_close.pending.borrow_mut().clear();
                nav_for_close.render_error();
                sync_bluetooth_discovery(
                    &sender_for_close,
                    &current_for_close,
                    &discovery_for_close,
                    false,
                );
            }
        });

        let escape = gtk::EventControllerKey::new();
        let motion_for_escape = motion.clone();
        escape.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                motion_for_escape.dismiss();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(escape);

        let media_widgets = vec![overview_media, detail_media];
        let artwork = ArtworkController::new(&window, &media_widgets);
        let view = Self {
            window,
            motion,
            current,
            navigation,
            network_tile,
            bluetooth_tile,
            audio_tile,
            power_tile,
            quick_grid,
            quick_layout: Cell::new(None),
            volume_scales: vec![overview_volume_scale, detail_volume_scale],
            volume_values: vec![overview_volume_value, detail_volume_value],
            volume_buttons: vec![overview_volume_button, detail_volume_button],
            audio_output_list,
            audio_output_specs: RefCell::new(Vec::new()),
            brightness_rows: vec![overview_brightness_row, detail_brightness_row],
            brightness_scales: vec![overview_brightness_scale, detail_brightness_scale],
            brightness_values: vec![overview_brightness_value, detail_brightness_value],
            brightness_device,
            media_widgets,
            artwork,
            keyboard_summary,
            resources_summary,
            battery_summary_button,
            battery_summary,
            time_summary,
            network_page,
            bluetooth_page,
            audio_state,
            audio_detail,
            battery_hero,
            battery_state,
            battery_detail,
            power_profile,
            keyboard_state,
            keyboard_detail,
            keyboard_layout_list,
            keyboard_layout_specs: RefCell::new(Vec::new()),
            cpu_gauge,
            memory_gauge,
            time_page,
            timer_widgets: RefCell::new(BTreeMap::new()),
            clock_label: clock,
            errors,
            suppress_controls,
            bluetooth_discovery_active,
            timer_sender: action_sender,
        };
        view.navigation
            .navigate(ControlCenterFocus::Overview, false);
        view.update(spec);
        view
    }

    pub fn window(&self) -> &gtk::ApplicationWindow {
        &self.window
    }

    pub fn present(&self) {
        self.motion.present();
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            self.current_page() == ControlCenterFocus::Bluetooth,
        );
    }

    pub fn dismiss(&self) {
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            false,
        );
        self.motion.dismiss();
    }

    pub fn is_visible(&self) -> bool {
        self.motion.is_presented()
    }

    pub fn destroy(&self) {
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            false,
        );
        self.motion.destroy();
    }

    pub fn show_page(&self, focus: ControlCenterFocus) {
        self.navigation.navigate(focus, false);
    }

    pub fn current_page(&self) -> ControlCenterFocus {
        self.navigation.page.get()
    }

    pub fn media_player(&self) -> Option<String> {
        self.current
            .borrow()
            .media
            .as_ref()
            .map(|media| media.player.clone())
    }

    pub fn update(&self, spec: &ControlCenterSpec) {
        *self.current.borrow_mut() = spec.clone();
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            self.is_visible() && self.current_page() == ControlCenterFocus::Bluetooth,
        );
        self.suppress_controls.set(true);

        update_toggle_tile(&self.network_tile, &spec.network);
        update_toggle_tile(&self.bluetooth_tile, &spec.bluetooth);
        let quick_layout = (spec.network.available, spec.bluetooth.available);
        if self.quick_layout.get() != Some(quick_layout) {
            reflow_quick_grid(
                &self.quick_grid,
                [
                    (&self.network_tile.root, spec.network.available),
                    (&self.audio_tile.root, true),
                    (&self.bluetooth_tile.root, spec.bluetooth.available),
                    (&self.power_tile.root, true),
                ],
            );
            self.quick_layout.set(Some(quick_layout));
        }
        let audio_available = !spec.audio.outputs.is_empty() && spec.audio.percent.is_some();
        self.audio_tile.detail.set_label(
            &spec
                .audio
                .percent
                .map(|percent| {
                    format!(
                        "{} · {percent}%",
                        if spec.audio.muted {
                            "Muted"
                        } else {
                            &spec.audio.detail
                        }
                    )
                })
                .unwrap_or_else(|| spec.audio.detail.clone()),
        );
        set_enabled_class(&self.audio_tile.root, audio_available && !spec.audio.muted);
        self.audio_tile.toggle.set_active(!spec.audio.muted);
        self.audio_tile.toggle.set_sensitive(audio_available);
        update_action_tile(&self.power_tile, &spec.power);

        let volume = spec.audio.percent.unwrap_or_default();
        for scale in &self.volume_scales {
            scale.set_value(f64::from(volume));
            scale.set_sensitive(audio_available);
        }
        for label in &self.volume_values {
            label.set_label(
                &spec
                    .audio
                    .percent
                    .map(|percent| format!("{percent}%"))
                    .unwrap_or_else(|| "--".to_string()),
            );
        }
        for button in &self.volume_buttons {
            button.set_icon_name(volume_icon_name(spec.audio.muted, spec.audio.percent));
            button.set_tooltip_text(Some(if spec.audio.muted { "Unmute" } else { "Mute" }));
            button.set_sensitive(audio_available);
            if spec.audio.muted {
                button.add_css_class("muted");
            } else {
                button.remove_css_class("muted");
            }
        }

        if let Some(brightness) = spec.brightness.as_ref() {
            *self.brightness_device.borrow_mut() = Some(brightness.device.clone());
            for row in &self.brightness_rows {
                row.set_visible(true);
            }
            for scale in &self.brightness_scales {
                scale.set_value(f64::from(brightness.percent));
            }
            for label in &self.brightness_values {
                label.set_label(&format!("{}%", brightness.percent));
            }
        } else {
            *self.brightness_device.borrow_mut() = None;
            for row in &self.brightness_rows {
                row.set_visible(false);
            }
        }

        for media in &self.media_widgets {
            update_media(media, spec.media.as_ref());
        }
        self.artwork.show(spec.media.as_ref(), &self.media_widgets);

        self.keyboard_summary.set_label(&spec.keyboard.summary);
        self.resources_summary.set_label(&format!(
            "{} / {}",
            percent_label(spec.cpu_percent),
            percent_label(spec.memory_percent)
        ));
        self.battery_summary_button
            .set_visible(spec.battery_present);
        if spec.battery_present {
            self.battery_summary.set_label(
                &spec
                    .battery_percent
                    .map(|percent| format!("{percent}%"))
                    .unwrap_or_else(|| "--".to_string()),
            );
        }
        self.time_summary.set_label(
            &spec
                .timers
                .first()
                .map(timer_summary)
                .or_else(|| spec.calendar.as_ref().map(|event| event.title.clone()))
                .unwrap_or_else(|| spec.clock.clone()),
        );

        update_connectivity_page(&self.network_page.connectivity, &spec.network);
        update_network_traffic(&self.network_page, &spec.network_traffic);
        update_bluetooth_page(
            &self.bluetooth_page,
            &spec.bluetooth_manager,
            self.timer_sender.clone(),
        );
        self.audio_state
            .set_label(if spec.audio.outputs.is_empty() {
                "Unavailable"
            } else if spec.audio.muted {
                "Muted"
            } else {
                "Sound on"
            });
        self.audio_detail.set_label(&spec.audio.detail);
        reconcile_audio_outputs(
            &self.audio_output_list,
            &mut self.audio_output_specs.borrow_mut(),
            &spec.audio.outputs,
            self.timer_sender.clone(),
            self.errors.clone(),
            self.navigation.clone(),
        );

        self.battery_hero.set_visible(spec.battery_present);
        if spec.battery_present {
            self.battery_state.set_label(
                &spec
                    .battery_percent
                    .map(|percent| format!("{percent}%"))
                    .unwrap_or_else(|| "Reading battery".to_string()),
            );
            self.battery_detail.set_label(if spec.charging {
                "Charging"
            } else {
                "On battery"
            });
        }
        self.power_profile.set_label(&spec.power.detail);
        self.keyboard_state.set_label(&spec.keyboard.current);
        self.keyboard_detail.set_label(&spec.keyboard.detail);
        reconcile_keyboard_layouts(
            &self.keyboard_layout_list,
            &mut self.keyboard_layout_specs.borrow_mut(),
            &spec.keyboard.layouts,
            self.timer_sender.clone(),
            self.errors.clone(),
            self.navigation.clone(),
        );
        update_metric(&self.cpu_gauge, spec.cpu_percent);
        update_metric(&self.memory_gauge, spec.memory_percent);

        update_time_page(&self.time_page, spec);
        reconcile_timers(
            &self.time_page.timer_list,
            &self.time_page.timer_empty,
            &mut self.timer_widgets.borrow_mut(),
            &spec.timers,
            self.timer_sender.clone(),
            self.errors.clone(),
            self.navigation.clone(),
        );
        self.clock_label.set_label(&spec.clock);
        let unavailable_page = match self.navigation.page.get() {
            ControlCenterFocus::Network => !spec.network.available,
            ControlCenterFocus::Bluetooth => !spec.bluetooth.available,
            _ => false,
        };
        if unavailable_page {
            self.navigation.navigate(ControlCenterFocus::Overview, true);
        }
        self.suppress_controls.set(false);
    }

    pub fn handle_completion(&self, completion: &ActionCompletion) -> bool {
        let Some(focus) = focus_from_origin(&completion.origin) else {
            return false;
        };
        match &completion.result {
            ActionResult::Completed => self.errors.borrow_mut().retry(focus),
            ActionResult::Failed { detail, .. } => {
                self.errors.borrow_mut().record_failure(focus, detail);
                if completion.origin.ends_with(":open-luma") {
                    self.present();
                }
            }
        }
        self.navigation.finish_action(focus);
        true
    }
}

impl Drop for ControlCenterView {
    fn drop(&mut self) {
        self.motion.destroy();
    }
}

fn toggle_tile(
    icon_name: &str,
    title: &str,
    focus: ControlCenterFocus,
    navigation: &NavigationUi,
) -> ToggleTile {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    root.add_css_class("quick-tile");
    let body = tile_body(icon_name, title);
    connect_navigation(&body.button, focus, navigation);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    root.append(&body.button);
    root.append(&toggle);
    ToggleTile {
        root,
        icon: body.icon,
        title: body.title,
        detail: body.detail,
        toggle,
    }
}

fn action_tile(
    icon_name: &str,
    title: &str,
    focus: ControlCenterFocus,
    navigation: &NavigationUi,
) -> ActionTile {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    root.add_css_class("quick-tile");
    let body = tile_body(icon_name, title);
    connect_navigation(&body.button, focus, navigation);
    let action = icon_button("view-refresh-symbolic", "Cycle power profile");
    action.add_css_class("tile-action");
    root.append(&body.button);
    root.append(&action);
    ActionTile {
        root,
        icon: body.icon,
        title: body.title,
        detail: body.detail,
        action,
    }
}

fn tile_body(icon_name: &str, title: &str) -> TileBody {
    let button = gtk::Button::new();
    button.add_css_class("quick-tile-body");
    button.set_has_frame(false);
    button.set_hexpand(true);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("quick-tile-icon");
    row.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    let detail = gtk::Label::new(None);
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    detail.set_max_width_chars(20);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);
    text.append(&detail);
    row.append(&text);
    button.set_child(Some(&row));
    TileBody {
        button,
        icon,
        title,
        detail,
    }
}

fn connect_navigation(button: &gtk::Button, page: ControlCenterFocus, navigation: &NavigationUi) {
    let navigation = navigation.clone();
    button.connect_clicked(move |_| navigation.navigate(page, false));
}

fn section_eyebrow(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("section-eyebrow");
    label.set_xalign(0.0);
    label
}

fn summary_tile(icon_name: &str, title: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("summary-tile");
    button.set_has_frame(false);
    button.set_hexpand(true);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("summary-icon");
    let title = gtk::Label::new(Some(title));
    title.add_css_class("summary-label");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&icon);
    header.append(&title);
    let value = gtk::Label::new(None);
    value.add_css_class("summary-value");
    value.set_xalign(0.0);
    value.set_max_width_chars(12);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    column.append(&header);
    column.append(&value);
    button.set_child(Some(&column));
    (button, value)
}

fn network_page() -> NetworkPage {
    let connectivity = connectivity_page("network-wireless-symbolic", "Wi-Fi");
    let traffic_root = gtk::Box::new(gtk::Orientation::Vertical, 7);
    traffic_root.add_css_class("network-traffic");
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = section_eyebrow("LIVE TRAFFIC");
    title.set_hexpand(true);
    let interface = gtk::Label::new(None);
    interface.add_css_class("network-interface");
    interface.set_xalign(1.0);
    heading.append(&title);
    heading.append(&interface);
    let graphs = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    graphs.add_css_class("network-graphs");
    graphs.set_homogeneous(true);
    let download = traffic_graph("DOWNLOAD", "download");
    let upload = traffic_graph("UPLOAD", "upload");
    graphs.append(&download.root);
    graphs.append(&upload.root);
    traffic_root.append(&heading);
    traffic_root.append(&graphs);
    connectivity.root.append(&traffic_root);
    NetworkPage {
        connectivity,
        traffic_root,
        interface,
        download,
        upload,
    }
}

fn traffic_graph(title: &str, class_name: &str) -> TrafficGraph {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 5);
    root.add_css_class("network-graph-card");
    root.add_css_class(class_name);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("network-graph-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let value = gtk::Label::new(Some("--"));
    value.add_css_class("network-graph-value");
    value.set_xalign(1.0);
    header.append(&title);
    header.append(&value);
    let area = gtk::DrawingArea::new();
    area.add_css_class("network-graph");
    area.set_content_height(54);
    area.set_hexpand(true);
    let history = Rc::new(RefCell::new(Vec::<u64>::new()));
    let history_for_draw = history.clone();
    area.set_draw_func(move |area, context, width, height| {
        draw_traffic_graph(area, context, width, height, &history_for_draw.borrow());
    });
    root.append(&header);
    root.append(&area);
    TrafficGraph {
        root,
        value,
        area,
        history,
    }
}

fn draw_traffic_graph(
    area: &gtk::DrawingArea,
    context: &gtk::cairo::Context,
    width: i32,
    height: i32,
    history: &[u64],
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let width = f64::from(width);
    let height = f64::from(height);
    let bottom = height - 3.0;
    let color = area.color();

    context.set_line_width(1.0);
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        0.10,
    );
    for fraction in [1.0 / 3.0, 2.0 / 3.0] {
        context.move_to(0.0, height * fraction);
        context.line_to(width, height * fraction);
    }
    let _ = context.stroke();

    if history.is_empty() {
        return;
    }
    let peak = history.iter().copied().max().unwrap_or_default().max(1) as f64;
    let step = width / (NETWORK_GRAPH_SAMPLES.saturating_sub(1) as f64);
    let start_x = width - step * history.len().saturating_sub(1) as f64;
    let points = history
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let x = start_x + step * index as f64;
            let y = bottom - ((*sample as f64 / peak) * (height - 8.0));
            (x, y)
        })
        .collect::<Vec<_>>();

    context.move_to(points[0].0, bottom);
    for (x, y) in &points {
        context.line_to(*x, *y);
    }
    context.line_to(points.last().map(|point| point.0).unwrap_or(width), bottom);
    context.close_path();
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        0.13,
    );
    let _ = context.fill();

    context.move_to(points[0].0, points[0].1);
    for (x, y) in points.iter().skip(1) {
        context.line_to(*x, *y);
    }
    context.set_line_width(2.0);
    context.set_line_join(gtk::cairo::LineJoin::Round);
    context.set_line_cap(gtk::cairo::LineCap::Round);
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        0.92,
    );
    let _ = context.stroke();
}

fn connectivity_page(icon_name: &str, title: &str) -> ConnectivityPage {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("control-page");

    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero.add_css_class("detail-hero");
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("detail-hero-icon");
    hero.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let eyebrow = gtk::Label::new(Some(title));
    eyebrow.add_css_class("detail-eyebrow");
    eyebrow.set_xalign(0.0);
    let state = gtk::Label::new(None);
    state.add_css_class("detail-hero-title");
    state.set_xalign(0.0);
    let detail = gtk::Label::new(None);
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    detail.set_max_width_chars(40);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&eyebrow);
    text.append(&state);
    text.append(&detail);
    hero.append(&text);
    page.append(&hero);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("detail-action-row");
    let label = gtk::Label::new(Some(&format!("Enable {title}")));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    row.append(&label);
    row.append(&toggle);
    page.append(&row);
    ConnectivityPage {
        root: page,
        icon,
        eyebrow,
        state,
        detail,
        toggle_row: row,
        toggle_label: label,
        toggle,
    }
}

fn bluetooth_page() -> BluetoothPage {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.add_css_class("control-page");

    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero.add_css_class("detail-hero");
    hero.add_css_class("bluetooth-hero");
    let icon = gtk::Image::from_icon_name("bluetooth-symbolic");
    icon.add_css_class("detail-hero-icon");
    hero.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let eyebrow = gtk::Label::new(Some("Bluetooth"));
    eyebrow.add_css_class("detail-eyebrow");
    eyebrow.set_xalign(0.0);
    let state = gtk::Label::new(None);
    state.add_css_class("detail-hero-title");
    state.set_xalign(0.0);
    let detail = gtk::Label::new(None);
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    text.append(&eyebrow);
    text.append(&state);
    text.append(&detail);
    let toggle = gtk::Switch::new();
    toggle.set_valign(gtk::Align::Center);
    toggle.set_tooltip_text(Some("Turn Bluetooth on or off"));
    hero.append(&text);
    hero.append(&toggle);
    page.append(&hero);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 7);
    list.add_css_class("bluetooth-device-list");
    let scroll = gtk::ScrolledWindow::new();
    scroll.add_css_class("bluetooth-device-scroll");
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_min_content_height(120);
    scroll.set_max_content_height(420);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&list));
    page.append(&scroll);

    BluetoothPage {
        root: page,
        icon,
        state,
        detail,
        toggle,
        list,
        previous: RefCell::new(None),
    }
}

fn detail_hero(icon_name: &str, title: &str) -> (gtk::Box, gtk::Label, gtk::Label) {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    root.add_css_class("detail-hero");
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("detail-hero-icon");
    root.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let eyebrow = gtk::Label::new(Some(title));
    eyebrow.add_css_class("detail-eyebrow");
    eyebrow.set_xalign(0.0);
    let state = gtk::Label::new(None);
    state.add_css_class("detail-hero-title");
    state.set_xalign(0.0);
    state.set_max_width_chars(40);
    state.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let detail = gtk::Label::new(None);
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    detail.set_max_width_chars(40);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&eyebrow);
    text.append(&state);
    text.append(&detail);
    root.append(&text);
    (root, state, detail)
}

fn section_intro(icon_name: &str, title: &str, detail: &str) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    root.add_css_class("detail-hero");
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("detail-hero-icon");
    root.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("detail-hero-title");
    title.set_xalign(0.0);
    let detail = gtk::Label::new(Some(detail));
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    text.append(&title);
    text.append(&detail);
    root.append(&text);
    root
}

fn slider_row(icon_name: &str, tooltip: &str) -> (gtk::Box, gtk::Scale, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("control-slider-row");
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.add_css_class("control-slider-icon");
    icon.set_tooltip_text(Some(tooltip));
    row.append(&icon);
    let scale = control_scale();
    scale.set_tooltip_text(Some(tooltip));
    let value = gtk::Label::new(None);
    value.add_css_class("slider-value");
    value.set_xalign(0.5);
    row.append(&scale);
    row.append(&value);
    (row, scale, value)
}

fn volume_slider_row() -> (gtk::Box, gtk::Scale, gtk::Label, gtk::Button) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("control-slider-row");
    let mute = icon_button("audio-volume-high-symbolic", "Mute");
    mute.add_css_class("control-slider-icon");
    mute.add_css_class("volume-mute-button");
    row.append(&mute);
    let scale = control_scale();
    scale.set_tooltip_text(Some("Volume"));
    let value = gtk::Label::new(None);
    value.add_css_class("slider-value");
    value.set_xalign(0.5);
    row.append(&scale);
    row.append(&value);
    (row, scale, value, mute)
}

fn volume_icon_name(muted: bool, percent: Option<u8>) -> &'static str {
    if muted {
        return "audio-volume-muted-symbolic";
    }
    match percent.unwrap_or_default().min(100) {
        0 => "audio-volume-muted-symbolic",
        1..=33 => "audio-volume-low-symbolic",
        34..=66 => "audio-volume-medium-symbolic",
        _ => "audio-volume-high-symbolic",
    }
}

fn metric_row(title: &str) -> MetricGauge {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 8);
    row.add_css_class("metric-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("metric-header");
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let value = gtk::Label::new(None);
    value.add_css_class("metric-value");
    value.set_xalign(1.0);
    header.append(&title);
    header.append(&value);
    row.append(&header);

    let track = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    track.add_css_class("metric-gauge");
    track.set_homogeneous(true);
    let segments = (0..METRIC_SEGMENTS)
        .map(|_| {
            let segment = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            segment.add_css_class("metric-segment");
            segment.set_hexpand(true);
            track.append(&segment);
            segment
        })
        .collect();
    row.append(&track);

    MetricGauge {
        root: row,
        value,
        segments,
    }
}

fn reconcile_audio_outputs(
    list: &gtk::Box,
    previous: &mut Vec<AudioOutputControlSpec>,
    outputs: &[AudioOutputControlSpec],
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
) {
    if previous == outputs {
        return;
    }
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    *previous = outputs.to_vec();

    if outputs.is_empty() {
        let empty = gtk::Label::new(Some("No audio outputs available"));
        empty.add_css_class("empty-state");
        empty.set_xalign(0.0);
        list.append(&empty);
        return;
    }

    let handle = ActionHandle {
        sender,
        errors,
        navigation,
        focus: ControlCenterFocus::Audio,
    };
    for output in outputs {
        let row = gtk::Button::new();
        row.add_css_class("audio-output-row");
        row.set_has_frame(false);
        row.set_hexpand(true);
        row.set_tooltip_text(Some(&format!("Use {} as audio output", output.label)));
        if output.selected {
            row.add_css_class("selected");
        }

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = gtk::Image::from_icon_name(&output.icon_name);
        icon.add_css_class("audio-output-icon");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let label = gtk::Label::new(Some(&output.label));
        label.add_css_class("audio-output-title");
        label.set_xalign(0.0);
        label.set_max_width_chars(36);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let detail = gtk::Label::new(Some(&output.detail));
        detail.add_css_class("audio-output-detail");
        detail.set_xalign(0.0);
        detail.set_max_width_chars(42);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&label);
        text.append(&detail);
        let selected = gtk::Image::from_icon_name("object-select-symbolic");
        selected.add_css_class("audio-output-selected");
        selected.set_visible(output.selected);
        content.append(&icon);
        content.append(&text);
        content.append(&selected);
        row.set_child(Some(&content));

        if !output.selected {
            let handle = handle.clone();
            let sink_name = output.name.clone();
            row.connect_clicked(move |_| {
                handle.send(
                    "set-output",
                    ActionIntent::SetAudioOutput {
                        sink_name: sink_name.clone(),
                    },
                );
            });
        }
        list.append(&row);
    }
}

fn reconcile_keyboard_layouts(
    list: &gtk::Box,
    previous: &mut Vec<KeyboardLayoutControlSpec>,
    layouts: &[KeyboardLayoutControlSpec],
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
) {
    if previous == layouts {
        return;
    }
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    *previous = layouts.to_vec();

    if layouts.is_empty() {
        let empty = gtk::Label::new(Some("No keyboard layouts available"));
        empty.add_css_class("empty-state");
        empty.set_xalign(0.0);
        list.append(&empty);
        return;
    }

    let handle = ActionHandle {
        sender,
        errors,
        navigation,
        focus: ControlCenterFocus::Keyboard,
    };
    for layout in layouts {
        let row = gtk::Button::new();
        row.add_css_class("keyboard-layout-row");
        row.set_has_frame(false);
        row.set_hexpand(true);
        row.set_tooltip_text(Some(&format!("Switch to {}", layout.raw_name)));
        if layout.selected {
            row.add_css_class("selected");
        }

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let badge = gtk::Label::new(Some(&layout.title));
        badge.add_css_class("keyboard-layout-badge");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = gtk::Label::new(Some(&layout.title));
        title.add_css_class("keyboard-layout-title");
        title.set_xalign(0.0);
        let detail = gtk::Label::new(Some(&layout.detail));
        detail.add_css_class("keyboard-layout-detail");
        detail.set_xalign(0.0);
        text.append(&title);
        text.append(&detail);
        let selected = gtk::Image::from_icon_name("object-select-symbolic");
        selected.add_css_class("keyboard-layout-selected");
        selected.set_visible(layout.selected);
        content.append(&badge);
        content.append(&text);
        content.append(&selected);
        row.set_child(Some(&content));

        if !layout.selected {
            let handle = handle.clone();
            let index = layout.index;
            row.connect_clicked(move |_| {
                handle.send(
                    "select-layout",
                    ActionIntent::SelectKeyboardLayout { index },
                );
            });
        }
        list.append(&row);
    }
}

fn media_card(
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
    current: Rc<RefCell<ControlCenterSpec>>,
) -> MediaWidgets {
    let root = gtk::Overlay::new();
    root.add_css_class("media-card");
    root.add_css_class("missing-art");
    root.set_size_request(-1, 140);
    root.set_vexpand(false);
    root.set_overflow(gtk::Overflow::Hidden);

    let artwork = gtk::Picture::new();
    artwork.add_css_class("media-artwork");
    artwork.set_content_fit(gtk::ContentFit::Cover);
    artwork.set_can_shrink(true);
    root.set_child(Some(&artwork));

    let placeholder = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    placeholder.add_css_class("media-artwork-placeholder");
    placeholder.set_halign(gtk::Align::End);
    placeholder.set_valign(gtk::Align::Start);
    root.add_overlay(&placeholder);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.add_css_class("media-content");
    content.set_halign(gtk::Align::Fill);
    content.set_valign(gtk::Align::Fill);
    content.set_hexpand(true);
    content.set_vexpand(true);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.add_css_class("media-copy");
    text.set_hexpand(true);
    text.set_valign(gtk::Align::End);
    let title = gtk::Label::new(None);
    title.add_css_class("media-title");
    title.set_xalign(0.0);
    title.set_max_width_chars(24);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let artist = gtk::Label::new(None);
    artist.add_css_class("media-artist");
    artist.set_xalign(0.0);
    artist.set_max_width_chars(24);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);
    text.append(&artist);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.add_css_class("media-controls");
    controls.set_valign(gtk::Align::End);
    let previous = icon_button("media-skip-backward-symbolic", "Previous");
    previous.add_css_class("media-transport-secondary");
    let play = icon_button("media-playback-start-symbolic", "Play or pause");
    play.add_css_class("media-transport-primary");
    let next = icon_button("media-skip-forward-symbolic", "Next");
    next.add_css_class("media-transport-secondary");
    let handle = ActionHandle {
        sender,
        errors,
        navigation,
        focus: ControlCenterFocus::Audio,
    };
    for (button, action, intent) in [
        (&previous, "previous", MediaControlAction::Previous),
        (&play, "play-pause", MediaControlAction::PlayPause),
        (&next, "next", MediaControlAction::Next),
    ] {
        let handle = handle.clone();
        let current = current.clone();
        button.connect_clicked(move |_| {
            let Some(player) = current
                .borrow()
                .media
                .as_ref()
                .map(|media| media.player.clone())
            else {
                return;
            };
            handle.send(
                action,
                ActionIntent::ControlMedia {
                    player,
                    action: intent,
                },
            );
        });
    }
    controls.append(&previous);
    controls.append(&play);
    controls.append(&next);
    content.append(&text);
    content.append(&controls);
    root.add_overlay(&content);
    MediaWidgets {
        root,
        artwork,
        title,
        artist,
        play,
    }
}

fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.set_has_frame(false);
    button
}

fn control_scale() -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale
}

fn connect_toggle_controls<'a, I, F>(
    toggles: I,
    suppress: Rc<Cell<bool>>,
    current: Rc<RefCell<ControlCenterSpec>>,
    handle: ActionHandle,
    intent: F,
) where
    I: IntoIterator<Item = &'a gtk::Switch>,
    F: Fn(&ControlCenterSpec, bool) -> Option<ActionIntent> + Clone + 'static,
{
    for toggle in toggles {
        let suppress = suppress.clone();
        let current = current.clone();
        let handle = handle.clone();
        let intent = intent.clone();
        toggle.connect_active_notify(move |toggle| {
            if suppress.get() {
                return;
            }
            if let Some(intent) = intent(&current.borrow(), toggle.is_active()) {
                handle.send("toggle", intent);
            }
        });
    }
}

fn install_percent_debounce<F>(
    scale: &gtk::Scale,
    suppress: Rc<Cell<bool>>,
    handle: ActionHandle,
    action: &'static str,
    intent: F,
) where
    F: Fn(u8) -> ActionIntent + Clone + 'static,
{
    let pending = Rc::new(RefCell::new(None::<glib::SourceId>));
    scale.connect_value_changed(move |scale| {
        if suppress.get() {
            return;
        }
        handle.errors.borrow_mut().retry(handle.focus);
        handle.navigation.render_error();
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let percent = (scale.value().round() as u8).min(100);
        let handle = handle.clone();
        let pending_for_timeout = pending.clone();
        let intent = intent.clone();
        *pending.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(150),
            move || {
                handle.send(action, intent(percent));
                pending_for_timeout.borrow_mut().take();
            },
        ));
    });
}

fn update_toggle_tile(tile: &ToggleTile, spec: &QuickControlSpec) {
    tile.icon.set_icon_name(Some(&spec.icon_name));
    tile.title.set_label(&spec.label);
    tile.detail.set_label(&spec.detail);
    tile.root.set_visible(spec.available);
    tile.root.set_sensitive(spec.available);
    tile.toggle.set_active(spec.enabled);
    tile.toggle.set_visible(spec.toggle_available);
    tile.toggle.set_sensitive(spec.toggle_available);
    set_enabled_class(&tile.root, spec.enabled);
}

fn update_action_tile(tile: &ActionTile, spec: &QuickControlSpec) {
    tile.icon.set_icon_name(Some(&spec.icon_name));
    tile.title.set_label(&spec.label);
    tile.detail.set_label(&spec.detail);
    tile.root.set_visible(spec.available);
    tile.root.set_sensitive(spec.available);
    set_enabled_class(&tile.root, spec.enabled);
}

fn update_connectivity_page(page: &ConnectivityPage, spec: &QuickControlSpec) {
    page.icon.set_icon_name(Some(&spec.icon_name));
    page.eyebrow.set_label(&spec.label);
    page.state.set_label(if spec.toggle_available {
        if spec.enabled { "On" } else { "Off" }
    } else if spec.enabled {
        "Connected"
    } else {
        "Disconnected"
    });
    page.detail.set_label(&spec.detail);
    page.toggle_row.set_visible(spec.toggle_available);
    page.toggle_label
        .set_label(&format!("Enable {}", spec.label));
    page.toggle.set_active(spec.enabled);
    page.toggle.set_sensitive(spec.toggle_available);
}

fn update_bluetooth_page(
    page: &BluetoothPage,
    spec: &BluetoothManagerSpec,
    sender: Sender<ActionRequest>,
) {
    page.icon.set_icon_name(Some(if spec.powered {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-disabled-symbolic"
    }));
    page.state
        .set_label(if spec.powered { "On" } else { "Off" });
    page.detail.set_label(if !spec.powered {
        "Turn on Bluetooth to find devices"
    } else if spec.discovering {
        match spec.connected_count {
            0 => "Scanning for nearby devices…",
            1 => "1 connected · Scanning…",
            count => {
                return page
                    .detail
                    .set_label(&format!("{count} connected · Scanning…"));
            }
        }
    } else {
        match spec.connected_count {
            0 => "No devices connected",
            1 => "1 device connected",
            count => return page.detail.set_label(&format!("{count} devices connected")),
        }
    });
    page.toggle.set_active(spec.powered);
    page.toggle.set_sensitive(spec.available);

    if page.previous.borrow().as_ref() == Some(spec) {
        return;
    }
    *page.previous.borrow_mut() = Some(spec.clone());
    while let Some(child) = page.list.first_child() {
        page.list.remove(&child);
    }

    if let Some(prompt) = spec.prompt.as_ref() {
        page.list.append(&bluetooth_pairing_prompt(prompt, sender));
        return;
    }
    if !spec.powered {
        let empty = gtk::Label::new(Some("Bluetooth is off"));
        empty.add_css_class("empty-state");
        empty.set_xalign(0.0);
        page.list.append(&empty);
        return;
    }
    if let Some(error) = spec.error.as_deref() {
        let error = gtk::Label::new(Some(error));
        error.add_css_class("bluetooth-error");
        error.set_wrap(true);
        error.set_xalign(0.0);
        page.list.append(&error);
    }

    let connected = spec
        .devices
        .iter()
        .filter(|device| device.connected)
        .cloned()
        .collect::<Vec<_>>();
    let saved = spec
        .devices
        .iter()
        .filter(|device| device.paired && !device.connected)
        .cloned()
        .collect::<Vec<_>>();
    let nearby = spec
        .devices
        .iter()
        .filter(|device| !device.paired && !device.connected)
        .cloned()
        .collect::<Vec<_>>();
    append_bluetooth_section(&page.list, "CONNECTED", &connected, sender.clone());
    append_bluetooth_section(&page.list, "SAVED DEVICES", &saved, sender.clone());
    if !nearby.is_empty() {
        append_bluetooth_section(&page.list, "NEARBY", &nearby, sender);
    } else {
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        heading.add_css_class("bluetooth-section-heading");
        let label = section_eyebrow("NEARBY");
        label.set_hexpand(true);
        heading.append(&label);
        if spec.discovering {
            let spinner = gtk::Spinner::new();
            spinner.add_css_class("bluetooth-scan-spinner");
            spinner.start();
            heading.append(&spinner);
        }
        page.list.append(&heading);
        let empty = gtk::Label::new(Some(if spec.discovering {
            "Looking for devices…"
        } else {
            "No nearby devices found"
        }));
        empty.add_css_class("empty-state");
        empty.set_xalign(0.0);
        page.list.append(&empty);
    }
}

fn append_bluetooth_section(
    list: &gtk::Box,
    title: &str,
    devices: &[BluetoothDeviceControlSpec],
    sender: Sender<ActionRequest>,
) {
    if devices.is_empty() {
        return;
    }
    list.append(&section_eyebrow(title));
    for device in devices {
        list.append(&bluetooth_device_row(device, sender.clone()));
    }
}

fn bluetooth_device_row(
    device: &BluetoothDeviceControlSpec,
    sender: Sender<ActionRequest>,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    row.add_css_class("bluetooth-device-row");
    if device.connected {
        row.add_css_class("connected");
    }
    if device.error.is_some() {
        row.add_css_class("has-error");
    }

    let primary = gtk::Button::new();
    primary.add_css_class("bluetooth-device-primary");
    primary.set_has_frame(false);
    primary.set_hexpand(true);
    primary.set_sensitive(device.operation.is_none());
    let accessible_action = if device.connected {
        "Disconnect"
    } else if device.paired {
        "Connect"
    } else {
        "Pair"
    };
    let accessible_label = format!("{accessible_action} {}", device.name);
    primary.update_property(&[
        gtk::accessible::Property::Label(&accessible_label),
        gtk::accessible::Property::Description(&device.detail),
    ]);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = gtk::Image::from_icon_name(&device.icon_name);
    icon.add_css_class("bluetooth-device-icon");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let name = gtk::Label::new(Some(&device.name));
    name.add_css_class("bluetooth-device-title");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let detail_text = device.error.as_deref().unwrap_or(&device.detail);
    let detail = gtk::Label::new(Some(detail_text));
    detail.add_css_class("bluetooth-device-detail");
    detail.set_xalign(0.0);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&name);
    text.append(&detail);
    content.append(&icon);
    content.append(&text);
    if device.operation.is_some() {
        let spinner = gtk::Spinner::new();
        spinner.start();
        spinner.add_css_class("bluetooth-device-spinner");
        content.append(&spinner);
    } else if device.connected {
        let connected = gtk::Image::from_icon_name("object-select-symbolic");
        connected.add_css_class("bluetooth-device-connected");
        content.append(&connected);
    }
    primary.set_child(Some(&content));

    let address = device.address.clone();
    let connected = device.connected;
    let paired = device.paired;
    let sender_for_primary = sender.clone();
    primary.connect_clicked(move |_| {
        let intent = if connected {
            ActionIntent::DisconnectBluetoothDevice {
                address: address.clone(),
            }
        } else if paired {
            ActionIntent::ConnectBluetoothDevice {
                address: address.clone(),
            }
        } else {
            ActionIntent::PairBluetoothDevice {
                address: address.clone(),
            }
        };
        send_bluetooth_action(&sender_for_primary, "device", intent);
    });
    row.append(&primary);

    if device.paired {
        let forget = icon_button("user-trash-symbolic", &format!("Forget {}", device.name));
        forget.add_css_class("bluetooth-forget-button");
        forget.set_sensitive(device.operation.is_none());
        let row_for_confirm = row.clone();
        let sender_for_forget = sender.clone();
        let address = device.address.clone();
        let name = device.name.clone();
        forget.connect_clicked(move |_| {
            while let Some(child) = row_for_confirm.first_child() {
                row_for_confirm.remove(&child);
            }
            row_for_confirm.add_css_class("confirming");
            let question = gtk::Label::new(Some(&format!("Forget {name}?")));
            question.set_xalign(0.0);
            question.set_hexpand(true);
            let cancel = gtk::Button::with_label("Cancel");
            cancel.add_css_class("compact-action");
            let forget_confirm = gtk::Button::with_label("Forget");
            forget_confirm.add_css_class("destructive-action");
            let row_for_cancel = row_for_confirm.clone();
            cancel.connect_clicked(move |_| {
                row_for_cancel.set_visible(false);
            });
            let sender = sender_for_forget.clone();
            let address = address.clone();
            forget_confirm.connect_clicked(move |_| {
                send_bluetooth_action(
                    &sender,
                    "forget",
                    ActionIntent::ForgetBluetoothDevice {
                        address: address.clone(),
                    },
                );
            });
            row_for_confirm.append(&question);
            row_for_confirm.append(&cancel);
            row_for_confirm.append(&forget_confirm);
        });
        row.append(&forget);
    }
    row
}

fn bluetooth_pairing_prompt(
    prompt: &BluetoothPairingPrompt,
    sender: Sender<ActionRequest>,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("bluetooth-pairing-card");
    let eyebrow = section_eyebrow("PAIRING REQUEST");
    let title = gtk::Label::new(Some(&prompt.device_name));
    title.add_css_class("bluetooth-pairing-title");
    title.set_xalign(0.0);
    let instruction = gtk::Label::new(None);
    instruction.add_css_class("supporting-text");
    instruction.set_wrap(true);
    instruction.set_xalign(0.0);
    card.append(&eyebrow);
    card.append(&title);
    card.append(&instruction);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    actions.set_halign(gtk::Align::End);
    let reject = gtk::Button::with_label("Cancel");
    reject.add_css_class("compact-action");
    let sender_for_reject = sender.clone();
    let address = prompt.address.clone();
    reject.connect_clicked(move |_| {
        send_bluetooth_action(
            &sender_for_reject,
            "cancel-pairing",
            ActionIntent::CancelBluetoothPairing {
                address: address.clone(),
            },
        );
    });
    actions.append(&reject);

    match &prompt.kind {
        BluetoothPairingPromptKind::ConfirmPasskey { passkey } => {
            instruction.set_label("Confirm that this code matches the device:");
            let code = gtk::Label::new(Some(&format!("{passkey:06}")));
            code.add_css_class("bluetooth-pairing-code");
            card.append(&code);
            let accept = gtk::Button::with_label("Pair");
            accept.add_css_class("primary-action");
            connect_pairing_response(&accept, sender, prompt.id, BluetoothPairingResponse::Accept);
            actions.append(&accept);
        }
        BluetoothPairingPromptKind::EnterPinCode => {
            instruction.set_label("Enter the PIN shown by the device.");
            let entry = gtk::Entry::new();
            entry.set_placeholder_text(Some("PIN"));
            entry.set_max_length(16);
            card.append(&entry);
            let accept = gtk::Button::with_label("Continue");
            accept.add_css_class("primary-action");
            accept.set_sensitive(false);
            let accept_for_change = accept.clone();
            entry.connect_changed(move |entry| {
                let value = entry.text();
                accept_for_change.set_sensitive(
                    !value.is_empty()
                        && value.len() <= 16
                        && value
                            .chars()
                            .all(|character| character.is_ascii_alphanumeric()),
                );
            });
            let sender = sender.clone();
            let entry_for_click = entry.clone();
            let prompt_id = prompt.id;
            accept.connect_clicked(move |_| {
                send_bluetooth_action(
                    &sender,
                    "pairing-response",
                    ActionIntent::RespondBluetoothPairing {
                        prompt_id,
                        response: BluetoothPairingResponse::PinCode(
                            entry_for_click.text().to_string(),
                        ),
                    },
                );
            });
            actions.append(&accept);
        }
        BluetoothPairingPromptKind::EnterPasskey => {
            instruction.set_label("Enter the six-digit passkey shown by the device.");
            let entry = gtk::Entry::new();
            entry.set_placeholder_text(Some("000000"));
            entry.set_max_length(6);
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            card.append(&entry);
            let accept = gtk::Button::with_label("Continue");
            accept.add_css_class("primary-action");
            accept.set_sensitive(false);
            let accept_for_change = accept.clone();
            entry.connect_changed(move |entry| {
                let value = entry.text();
                accept_for_change.set_sensitive(
                    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit()),
                );
            });
            let sender = sender.clone();
            let entry_for_click = entry.clone();
            let prompt_id = prompt.id;
            accept.connect_clicked(move |_| {
                if let Ok(passkey) = entry_for_click.text().parse::<u32>() {
                    send_bluetooth_action(
                        &sender,
                        "pairing-response",
                        ActionIntent::RespondBluetoothPairing {
                            prompt_id,
                            response: BluetoothPairingResponse::Passkey(passkey),
                        },
                    );
                }
            });
            actions.append(&accept);
        }
        BluetoothPairingPromptKind::DisplayPinCode { pin_code } => {
            instruction.set_label("Enter this PIN on the device, then wait for it to connect:");
            let code = gtk::Label::new(Some(pin_code));
            code.add_css_class("bluetooth-pairing-code");
            card.append(&code);
        }
        BluetoothPairingPromptKind::DisplayPasskey { passkey, entered } => {
            instruction.set_label(&format!(
                "Type this code on the device · {entered}/6 digits entered"
            ));
            let code = gtk::Label::new(Some(&format!("{passkey:06}")));
            code.add_css_class("bluetooth-pairing-code");
            card.append(&code);
        }
        BluetoothPairingPromptKind::Authorize => {
            instruction.set_label("Allow this device to pair with this computer?");
            let accept = gtk::Button::with_label("Allow");
            accept.add_css_class("primary-action");
            connect_pairing_response(&accept, sender, prompt.id, BluetoothPairingResponse::Accept);
            actions.append(&accept);
        }
    }
    card.append(&actions);
    card
}

fn connect_pairing_response(
    button: &gtk::Button,
    sender: Sender<ActionRequest>,
    prompt_id: u64,
    response: BluetoothPairingResponse,
) {
    button.connect_clicked(move |_| {
        send_bluetooth_action(
            &sender,
            "pairing-response",
            ActionIntent::RespondBluetoothPairing {
                prompt_id,
                response: response.clone(),
            },
        );
    });
}

fn send_bluetooth_action(sender: &Sender<ActionRequest>, action: &str, intent: ActionIntent) {
    let _ = sender.send(ActionRequest {
        origin: control_center_origin(ControlCenterFocus::Bluetooth, action),
        intent,
    });
}

fn sync_bluetooth_discovery(
    sender: &Sender<ActionRequest>,
    current: &Rc<RefCell<ControlCenterSpec>>,
    active: &Rc<Cell<bool>>,
    page_visible: bool,
) {
    let spec = current.borrow();
    let desired =
        page_visible && spec.bluetooth_manager.available && spec.bluetooth_manager.powered;
    if !page_visible && let Some(prompt) = spec.bluetooth_manager.prompt.as_ref() {
        send_bluetooth_action(
            sender,
            "cancel-pairing",
            ActionIntent::CancelBluetoothPairing {
                address: prompt.address.clone(),
            },
        );
    }
    drop(spec);
    if active.get() == desired {
        return;
    }
    send_bluetooth_action(
        sender,
        if desired { "start-scan" } else { "stop-scan" },
        ActionIntent::SetBluetoothDiscovery { enabled: desired },
    );
    active.set(desired);
}

fn update_network_traffic(page: &NetworkPage, spec: &NetworkTrafficSpec) {
    page.traffic_root.set_visible(spec.interface.is_some());
    page.interface
        .set_label(spec.interface.as_deref().unwrap_or_default());
    update_traffic_graph(
        &page.download,
        spec.download_bytes_per_second,
        &spec.download_history,
    );
    update_traffic_graph(
        &page.upload,
        spec.upload_bytes_per_second,
        &spec.upload_history,
    );
}

fn update_traffic_graph(graph: &TrafficGraph, rate: Option<u64>, history: &[u64]) {
    graph.value.set_label(&format_network_rate(rate));
    *graph.history.borrow_mut() = history.to_vec();
    graph.area.queue_draw();
}

fn format_network_rate(rate: Option<u64>) -> String {
    let Some(rate) = rate else {
        return "--".to_string();
    };
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * KIB;
    const GIB: f64 = 1024.0 * MIB;
    let rate = rate as f64;
    if rate < KIB {
        format!("{rate:.0} B/s")
    } else if rate < MIB {
        format_rate_unit(rate / KIB, "KB/s")
    } else if rate < GIB {
        format_rate_unit(rate / MIB, "MB/s")
    } else {
        format_rate_unit(rate / GIB, "GB/s")
    }
}

fn format_rate_unit(value: f64, unit: &str) -> String {
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

fn quick_grid_placements(count: usize) -> Vec<(i32, i32, i32)> {
    (0..count)
        .map(|index| {
            let column = i32::try_from(index % 2).unwrap_or_default();
            let row = i32::try_from(index / 2).unwrap_or_default();
            let width = if count % 2 == 1 && index + 1 == count {
                2
            } else {
                1
            };
            (column, row, width)
        })
        .collect()
}

fn reflow_quick_grid<'a>(grid: &gtk::Grid, tiles: impl IntoIterator<Item = (&'a gtk::Box, bool)>) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    let tiles = tiles
        .into_iter()
        .filter_map(|(tile, visible)| visible.then_some(tile))
        .collect::<Vec<_>>();
    let placements = quick_grid_placements(tiles.len());
    for (tile, (column, row, width)) in tiles.into_iter().zip(placements) {
        grid.attach(tile, column, row, width, 1);
    }
}

fn update_media(widgets: &MediaWidgets, spec: Option<&MediaControlSpec>) {
    if let Some(media) = spec {
        widgets.title.set_label(&media.title);
        widgets.artist.set_label(&media.artist);
        widgets.play.set_icon_name(if media.playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        widgets.root.set_visible(true);
    } else {
        widgets.root.set_visible(false);
    }
}

fn build_time_page(
    spec: &ControlCenterSpec,
    action_sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
    calendar_sender: Option<Sender<CalendarMonthRequest>>,
) -> TimePage {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("control-page");
    root.add_css_class("time-page");

    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero.add_css_class("time-hero");
    let icon = gtk::Image::from_icon_name("preferences-system-time-symbolic");
    icon.add_css_class("time-hero-icon");
    icon.set_pixel_size(26);
    hero.append(&icon);
    let hero_text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    hero_text.set_hexpand(true);
    let clock = gtk::Label::new(Some(&spec.clock));
    clock.add_css_class("time-hero-clock");
    clock.set_xalign(0.0);
    let date = gtk::Label::new(None);
    date.add_css_class("time-hero-date");
    date.set_xalign(0.0);
    hero_text.append(&clock);
    hero_text.append(&date);
    hero.append(&hero_text);
    let today = gtk::Button::with_label("Today");
    today.add_css_class("compact-action");
    hero.append(&today);
    root.append(&hero);

    let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    tabs.add_css_class("time-tabs");
    tabs.set_homogeneous(true);
    let calendar_tab = gtk::ToggleButton::with_label("Calendar");
    let timers_tab = gtk::ToggleButton::with_label("Timers");
    calendar_tab.add_css_class("time-tab");
    timers_tab.add_css_class("time-tab");
    timers_tab.set_group(Some(&calendar_tab));
    calendar_tab.set_active(true);
    tabs.append(&calendar_tab);
    tabs.append(&timers_tab);
    root.append(&tabs);

    let view_stack = gtk::Stack::new();
    view_stack.add_css_class("time-view-stack");
    view_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    view_stack.set_transition_duration(160);
    view_stack.set_hhomogeneous(true);
    view_stack.set_vhomogeneous(false);

    let calendar_view = gtk::Box::new(gtk::Orientation::Vertical, 8);
    calendar_view.add_css_class("time-calendar-view");
    let calendar = gtk::Calendar::new();
    calendar.add_css_class("time-calendar");
    calendar.set_show_day_names(true);
    calendar.set_show_heading(true);
    calendar.set_show_week_numbers(false);
    calendar_view.append(&calendar);

    let agenda_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let agenda_title = gtk::Label::new(Some("Selected day"));
    agenda_title.add_css_class("section-title");
    agenda_title.set_xalign(0.0);
    agenda_title.set_hexpand(true);
    let open_calendar = gtk::Button::with_label("Open calendar");
    open_calendar.add_css_class("compact-action");
    agenda_heading.append(&agenda_title);
    agenda_heading.append(&open_calendar);
    calendar_view.append(&agenda_heading);

    let agenda_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    agenda_list.add_css_class("agenda-list");
    let agenda_empty = gtk::Label::new(Some("No events on this day"));
    agenda_empty.add_css_class("empty-state");
    agenda_empty.set_xalign(0.0);
    let agenda_status = gtk::Label::new(Some("Loading calendar…"));
    agenda_status.add_css_class("calendar-status");
    agenda_status.set_xalign(0.0);
    let agenda_scroll = gtk::ScrolledWindow::new();
    agenda_scroll.add_css_class("agenda-scroll");
    agenda_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    agenda_scroll.set_min_content_height(150);
    agenda_scroll.set_max_content_height(190);
    agenda_scroll.set_propagate_natural_height(true);
    let agenda_content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    agenda_content.append(&agenda_status);
    agenda_content.append(&agenda_empty);
    agenda_content.append(&agenda_list);
    agenda_scroll.set_child(Some(&agenda_content));
    calendar_view.append(&agenda_scroll);
    view_stack.add_named(&calendar_view, Some("calendar"));

    let timers_view = gtk::Box::new(gtk::Orientation::Vertical, 9);
    timers_view.add_css_class("time-timers-view");
    timers_view.append(&section_eyebrow("QUICK START"));
    let presets = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    presets.add_css_class("timer-presets");
    presets.set_homogeneous(true);
    let timer_action = ActionHandle {
        sender: action_sender.clone(),
        errors: errors.clone(),
        navigation: navigation.clone(),
        focus: ControlCenterFocus::Clock,
    };
    for minutes in [5_u64, 15, 25, 45] {
        let button = gtk::Button::with_label(&format!("{minutes} min"));
        button.add_css_class("timer-preset");
        let handle = timer_action.clone();
        button.connect_clicked(move |_| {
            handle.send(
                "start-timer",
                ActionIntent::StartTimer {
                    label: format!("{minutes} minute timer"),
                    duration_seconds: minutes * 60,
                },
            );
        });
        presets.append(&button);
    }
    timers_view.append(&presets);
    timers_view.append(&section_eyebrow("CUSTOM TIMER"));
    let composer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    composer.add_css_class("timer-composer");
    let timer_label = gtk::Entry::new();
    timer_label.set_placeholder_text(Some("Timer label"));
    timer_label.set_hexpand(true);
    let duration = gtk::SpinButton::with_range(1.0, 720.0, 1.0);
    duration.set_value(25.0);
    duration.set_tooltip_text(Some("Duration in minutes"));
    duration.set_width_chars(4);
    let minutes_label = gtk::Label::new(Some("min"));
    minutes_label.add_css_class("supporting-text");
    let start = gtk::Button::with_label("Start");
    start.add_css_class("primary-action");
    composer.append(&timer_label);
    composer.append(&duration);
    composer.append(&minutes_label);
    composer.append(&start);
    timers_view.append(&composer);

    let active_heading = gtk::Label::new(Some("Active timers"));
    active_heading.add_css_class("section-title");
    active_heading.set_xalign(0.0);
    timers_view.append(&active_heading);
    let timer_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let timer_empty = gtk::Label::new(Some("No active timers"));
    timer_empty.add_css_class("empty-state");
    timer_empty.set_xalign(0.0);
    timer_list.append(&timer_empty);
    let timer_scroll = gtk::ScrolledWindow::new();
    timer_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    timer_scroll.set_min_content_height(170);
    timer_scroll.set_max_content_height(250);
    timer_scroll.set_propagate_natural_height(true);
    timer_scroll.set_child(Some(&timer_list));
    timers_view.append(&timer_scroll);
    view_stack.add_named(&timers_view, Some("timers"));
    view_stack.set_visible_child_name("calendar");
    root.append(&view_stack);

    let calendar_action = ActionHandle {
        sender: action_sender,
        errors,
        navigation,
        focus: ControlCenterFocus::Clock,
    };
    let open_handle = calendar_action.clone();
    open_calendar.connect_clicked(move |_| {
        open_handle.send("open-calendar", ActionIntent::OpenCalendar);
    });

    let stack_for_calendar = view_stack.clone();
    calendar_tab.connect_toggled(move |button| {
        if button.is_active() {
            stack_for_calendar.set_visible_child_name("calendar");
        }
    });
    let stack_for_timers = view_stack.clone();
    timers_tab.connect_toggled(move |button| {
        if button.is_active() {
            stack_for_timers.set_visible_child_name("timers");
        }
    });

    let custom_handle = timer_action;
    let label_for_start = timer_label.clone();
    let duration_for_start = duration.clone();
    start.connect_clicked(move |_| {
        let minutes = u64::try_from(duration_for_start.value_as_int())
            .unwrap_or(1)
            .clamp(1, 720);
        let raw_label = label_for_start.text();
        let label = if raw_label.trim().is_empty() {
            "Timer".to_string()
        } else {
            raw_label.trim().to_string()
        };
        custom_handle.send(
            "start-timer",
            ActionIntent::StartTimer {
                label,
                duration_seconds: minutes * 60,
            },
        );
        label_for_start.set_text("");
    });

    let page = TimePage {
        root,
        clock,
        date,
        view_stack,
        calendar_tab,
        calendar,
        agenda_list,
        agenda_empty,
        agenda_status,
        rendered_agenda: Rc::new(RefCell::new(None)),
        expanded_event: Rc::new(RefCell::new(None)),
        agenda_details: Rc::new(RefCell::new(BTreeMap::new())),
        latest_spec: Rc::new(RefCell::new(spec.clone())),
        calendar_action,
        calendar_sender,
        timer_list,
        timer_empty,
    };

    connect_time_calendar(&page);
    let page_for_today = page.clone();
    today.connect_clicked(move |_| reset_time_calendar(&page_for_today));
    request_calendar_month(&page);
    page
}

fn connect_time_calendar(page: &TimePage) {
    let latest = page.latest_spec.clone();
    let list = page.agenda_list.clone();
    let empty = page.agenda_empty.clone();
    let status = page.agenda_status.clone();
    let rendered = page.rendered_agenda.clone();
    let expanded = page.expanded_event.clone();
    let details = page.agenda_details.clone();
    let action = page.calendar_action.clone();
    page.calendar.connect_day_selected(move |calendar| {
        render_agenda(
            calendar,
            &list,
            &empty,
            &status,
            &rendered,
            &expanded,
            &details,
            &latest.borrow(),
            &action,
        );
    });

    macro_rules! connect_load {
        ($signal:ident) => {{
            let sender = page.calendar_sender.clone();
            let status = page.agenda_status.clone();
            let empty = page.agenda_empty.clone();
            let list = page.agenda_list.clone();
            let rendered = page.rendered_agenda.clone();
            page.calendar.$signal(move |calendar| {
                begin_calendar_load(calendar, &sender, &status, &empty, &list, &rendered);
            });
        }};
    }
    connect_load!(connect_next_month);
    connect_load!(connect_prev_month);
    connect_load!(connect_next_year);
    connect_load!(connect_prev_year);
}

fn begin_calendar_load(
    calendar: &gtk::Calendar,
    sender: &Option<Sender<CalendarMonthRequest>>,
    status: &gtk::Label,
    empty: &gtk::Label,
    list: &gtk::Box,
    rendered: &Rc<RefCell<Option<AgendaRenderKey>>>,
) {
    calendar.clear_marks();
    status.set_label(if sender.is_some() {
        "Loading calendar…"
    } else {
        "Calendar integration unavailable"
    });
    status.set_visible(true);
    empty.set_visible(false);
    clear_box(list);
    rendered.borrow_mut().take();
    if let Some(sender) = sender {
        let _ = sender.send(CalendarMonthRequest {
            year: calendar.year(),
            month: u32::try_from(calendar.month()).unwrap_or(1),
        });
    }
}

fn request_calendar_month(page: &TimePage) {
    begin_calendar_load(
        &page.calendar,
        &page.calendar_sender,
        &page.agenda_status,
        &page.agenda_empty,
        &page.agenda_list,
        &page.rendered_agenda,
    );
}

fn reset_time_page(page: &TimePage) {
    page.calendar_tab.set_active(true);
    page.view_stack.set_visible_child_name("calendar");
    reset_time_calendar(page);
}

fn reset_time_calendar(page: &TimePage) {
    if let Ok(today) = glib::DateTime::now_local() {
        page.calendar.set_date(&today);
    }
    request_calendar_month(page);
}

fn update_time_page(page: &TimePage, spec: &ControlCenterSpec) {
    *page.latest_spec.borrow_mut() = spec.clone();
    page.clock.set_label(&spec.clock);
    page.date.set_label(&local_date_label(spec.clock_epoch));
    update_calendar_marks(&page.calendar, spec.calendar_agenda.as_ref());
    render_agenda(
        &page.calendar,
        &page.agenda_list,
        &page.agenda_empty,
        &page.agenda_status,
        &page.rendered_agenda,
        &page.expanded_event,
        &page.agenda_details,
        spec,
        &page.calendar_action,
    );
}

fn local_date_label(epoch: i64) -> String {
    glib::DateTime::from_unix_local(epoch)
        .and_then(|date| date.format("%A, %e %B"))
        .map(|label| label.trim().to_string())
        .unwrap_or_else(|_| "Local time".to_string())
}

fn update_calendar_marks(calendar: &gtk::Calendar, agenda: Option<&CalendarAgenda>) {
    calendar.clear_marks();
    let Some(agenda) = agenda.filter(|agenda| {
        agenda.year == calendar.year()
            && agenda.month == u32::try_from(calendar.month()).unwrap_or_default()
    }) else {
        return;
    };
    for day in 1..=31 {
        if !events_for_local_day(&agenda.events, agenda.year, agenda.month, day).is_empty() {
            calendar.mark_day(u32::try_from(day).unwrap_or_default());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_agenda(
    calendar: &gtk::Calendar,
    list: &gtk::Box,
    empty: &gtk::Label,
    status: &gtk::Label,
    rendered: &Rc<RefCell<Option<AgendaRenderKey>>>,
    expanded: &Rc<RefCell<Option<String>>>,
    details: &Rc<RefCell<BTreeMap<String, gtk::Box>>>,
    spec: &ControlCenterSpec,
    action: &ActionHandle,
) {
    let year = calendar.year();
    let month = u32::try_from(calendar.month()).unwrap_or(1);
    let day = calendar.day();
    let matching = spec
        .calendar_agenda
        .as_ref()
        .filter(|agenda| agenda.year == year && agenda.month == month);
    let events = matching
        .map(|agenda| events_for_local_day(&agenda.events, year, month, day))
        .unwrap_or_default();
    let key = AgendaRenderKey {
        year,
        month,
        day,
        events: events.clone(),
        error: spec.calendar_agenda_error.clone(),
    };
    if rendered.borrow().as_ref() == Some(&key) {
        return;
    }
    *rendered.borrow_mut() = Some(key);
    clear_box(list);
    details.borrow_mut().clear();
    expanded.borrow_mut().take();

    if matching.is_none() {
        status.set_label(
            spec.calendar_agenda_error
                .as_deref()
                .unwrap_or("Loading calendar…"),
        );
        status.set_visible(true);
        empty.set_visible(false);
        return;
    }
    status.set_visible(false);
    empty.set_visible(events.is_empty());
    for event in events {
        list.append(&agenda_event_row(&event, expanded, details, action));
    }
}

fn events_for_local_day(
    events: &[CalendarAgendaEvent],
    year: i32,
    month: u32,
    day: i32,
) -> Vec<CalendarAgendaEvent> {
    let Ok(start) =
        glib::DateTime::from_local(year, i32::try_from(month).unwrap_or(1), day, 0, 0, 0.0)
    else {
        return Vec::new();
    };
    let Ok(end) = start.add_days(1) else {
        return Vec::new();
    };
    let start_epoch = start.to_unix();
    let end_epoch = end.to_unix();
    let mut matching = events
        .iter()
        .filter(|event| {
            let event_end = if event.end_epoch == event.start_epoch {
                event.start_epoch.saturating_add(1)
            } else {
                event.end_epoch
            };
            event.start_epoch < end_epoch && event_end > start_epoch
        })
        .cloned()
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        (!left.all_day, left.start_epoch, &left.title).cmp(&(
            !right.all_day,
            right.start_epoch,
            &right.title,
        ))
    });
    matching
}

fn agenda_event_row(
    event: &CalendarAgendaEvent,
    expanded: &Rc<RefCell<Option<String>>>,
    details: &Rc<RefCell<BTreeMap<String, gtk::Box>>>,
    action: &ActionHandle,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("agenda-event");
    let header = gtk::Button::new();
    header.add_css_class("agenda-event-header");
    header.set_has_frame(false);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let time = gtk::Label::new(Some(&event_time_label(event)));
    time.add_css_class("agenda-event-time");
    time.set_width_chars(10);
    time.set_xalign(0.0);
    let title = gtk::Label::new(Some(&event.title));
    title.add_css_class("agenda-event-title");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let arrow = gtk::Image::from_icon_name("go-down-symbolic");
    arrow.add_css_class("agenda-event-arrow");
    row.append(&time);
    row.append(&title);
    row.append(&arrow);
    header.set_child(Some(&row));
    root.append(&header);

    let detail = gtk::Box::new(gtk::Orientation::Vertical, 5);
    detail.add_css_class("agenda-event-detail");
    detail.set_visible(false);
    let full_time = gtk::Label::new(Some(&event_full_time_label(event)));
    full_time.set_xalign(0.0);
    detail.append(&full_time);
    if let Some(location) = event.location.as_deref() {
        let location = gtk::Label::new(Some(&format!("Location · {location}")));
        location.add_css_class("supporting-text");
        location.set_xalign(0.0);
        location.set_wrap(true);
        detail.append(&location);
    }
    if let Some(calendar) = event.calendar.as_deref() {
        let calendar = gtk::Label::new(Some(&format!("Calendar · {calendar}")));
        calendar.add_css_class("supporting-text");
        calendar.set_xalign(0.0);
        detail.append(&calendar);
    }
    let open = gtk::Button::with_label("Open in Evolution");
    open.add_css_class("compact-action");
    open.set_halign(gtk::Align::Start);
    let open_handle = action.clone();
    open.connect_clicked(move |_| open_handle.send("open-calendar", ActionIntent::OpenCalendar));
    detail.append(&open);
    root.append(&detail);
    details
        .borrow_mut()
        .insert(event.id.clone(), detail.clone());

    let event_id = event.id.clone();
    let expanded = expanded.clone();
    let details = details.clone();
    header.connect_clicked(move |_| {
        let next =
            (expanded.borrow().as_deref() != Some(event_id.as_str())).then_some(event_id.clone());
        for (id, detail) in details.borrow().iter() {
            detail.set_visible(next.as_deref() == Some(id.as_str()));
        }
        *expanded.borrow_mut() = next;
    });
    root
}

fn event_time_label(event: &CalendarAgendaEvent) -> String {
    if event.all_day {
        return "All day".to_string();
    }
    glib::DateTime::from_unix_local(event.start_epoch)
        .and_then(|date| date.format("%H:%M"))
        .map(|label| label.to_string())
        .unwrap_or_else(|_| "Timed".to_string())
}

fn event_full_time_label(event: &CalendarAgendaEvent) -> String {
    let Ok(start) = glib::DateTime::from_unix_local(event.start_epoch) else {
        return event_time_label(event);
    };
    if event.all_day {
        return start
            .format("%A, %e %B · All day")
            .map(|label| label.to_string())
            .unwrap_or_else(|_| "All day".to_string());
    }
    let start_label = start
        .format("%A, %e %B · %H:%M")
        .map(|label| label.to_string())
        .unwrap_or_else(|_| event_time_label(event));
    let end_label = glib::DateTime::from_unix_local(event.end_epoch)
        .and_then(|date| date.format("%H:%M"))
        .map(|label| label.to_string())
        .unwrap_or_default();
    if end_label.is_empty() {
        start_label
    } else {
        format!("{start_label}–{end_label}")
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn reconcile_timers(
    list: &gtk::Box,
    empty: &gtk::Label,
    widgets: &mut TimerWidgets,
    timers: &[TimerControlSpec],
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
) {
    let stale = widgets
        .keys()
        .filter(|id| !timers.iter().any(|timer| timer.id == **id))
        .cloned()
        .collect::<Vec<_>>();
    for id in stale {
        if let Some(row) = widgets.remove(&id) {
            list.remove(&row.root);
        }
    }

    for timer in timers {
        if !widgets.contains_key(&timer.id) {
            let row = timer_row(timer, sender.clone(), errors.clone(), navigation.clone());
            list.append(&row.root);
            widgets.insert(timer.id.clone(), row);
        }
        update_timer_row(widgets.get(&timer.id).expect("timer row"), timer);
    }
    empty.set_visible(timers.is_empty());
}

fn timer_row(
    timer: &TimerControlSpec,
    sender: Sender<ActionRequest>,
    errors: Rc<RefCell<ControlCenterErrors>>,
    navigation: NavigationUi,
) -> TimerRowWidgets {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    root.add_css_class("timer-row");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.set_max_width_chars(32);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let remaining = gtk::Label::new(None);
    remaining.add_css_class("supporting-text");
    remaining.set_xalign(0.0);
    text.append(&title);
    text.append(&remaining);
    root.append(&text);

    let current = Rc::new(RefCell::new(timer.clone()));
    let primary = icon_button("media-playback-pause-symbolic", "Pause timer");
    let primary_handle = ActionHandle {
        sender: sender.clone(),
        errors: errors.clone(),
        navigation: navigation.clone(),
        focus: ControlCenterFocus::Clock,
    };
    let current_for_primary = current.clone();
    primary.connect_clicked(move |_| {
        let timer = current_for_primary.borrow();
        let id = timer.id.clone();
        let (action, intent) = match timer_action_state(&timer) {
            TimerActionState::Clear => ("cancel-timer", ActionIntent::CancelTimer { id }),
            TimerActionState::Resume => ("resume-timer", ActionIntent::ResumeTimer { id }),
            TimerActionState::Pause => ("pause-timer", ActionIntent::PauseTimer { id }),
        };
        drop(timer);
        primary_handle.send(action, intent);
    });
    root.append(&primary);

    let cancel = icon_button("window-close-symbolic", "Cancel timer");
    let cancel_handle = ActionHandle {
        sender,
        errors,
        navigation,
        focus: ControlCenterFocus::Clock,
    };
    let current_for_cancel = current.clone();
    cancel.connect_clicked(move |_| {
        let id = current_for_cancel.borrow().id.clone();
        cancel_handle.send("cancel-timer", ActionIntent::CancelTimer { id });
    });
    root.append(&cancel);

    TimerRowWidgets {
        root,
        title,
        remaining,
        primary,
        cancel,
        current,
    }
}

fn update_timer_row(row: &TimerRowWidgets, timer: &TimerControlSpec) {
    *row.current.borrow_mut() = timer.clone();
    row.title.set_label(&timer.label);
    row.remaining.set_label(&timer_summary(timer));
    let (icon, tooltip) = match timer_action_state(timer) {
        TimerActionState::Clear => ("edit-delete-symbolic", "Clear timer"),
        TimerActionState::Resume => ("media-playback-start-symbolic", "Resume timer"),
        TimerActionState::Pause => ("media-playback-pause-symbolic", "Pause timer"),
    };
    row.primary.set_icon_name(icon);
    row.primary.set_tooltip_text(Some(tooltip));
    row.cancel.set_visible(!timer.completed);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerActionState {
    Clear,
    Resume,
    Pause,
}

fn timer_action_state(timer: &TimerControlSpec) -> TimerActionState {
    if timer.completed {
        TimerActionState::Clear
    } else if timer.paused {
        TimerActionState::Resume
    } else {
        TimerActionState::Pause
    }
}

fn timer_summary(timer: &TimerControlSpec) -> String {
    if timer.completed {
        "Complete".to_string()
    } else {
        let minutes = timer.remaining_seconds / 60;
        let seconds = timer.remaining_seconds % 60;
        let suffix = if timer.paused { " · paused" } else { "" };
        format!("{minutes}:{seconds:02}{suffix}")
    }
}

fn update_metric(gauge: &MetricGauge, percent: Option<u8>) {
    let visual = metric_visual(percent);
    gauge.value.set_label(&visual.label);

    for class in ["warning", "critical", "unavailable"] {
        gauge.root.remove_css_class(class);
    }
    match visual.level {
        MetricLevel::Normal => {}
        MetricLevel::Warning => gauge.root.add_css_class("warning"),
        MetricLevel::Critical => gauge.root.add_css_class("critical"),
        MetricLevel::Unavailable => gauge.root.add_css_class("unavailable"),
    }

    for (index, segment) in gauge.segments.iter().enumerate() {
        if index < visual.active_segments {
            segment.add_css_class("active");
        } else {
            segment.remove_css_class("active");
        }
    }
}

fn metric_visual(percent: Option<u8>) -> MetricVisual {
    let Some(percent) = percent else {
        return MetricVisual {
            active_segments: 0,
            level: MetricLevel::Unavailable,
            label: "Unavailable".to_string(),
        };
    };

    let percent = percent.min(100);
    let active_segments = if percent == 0 {
        0
    } else {
        usize::from(percent).div_ceil(100 / METRIC_SEGMENTS)
    };
    let level = match percent {
        90..=100 => MetricLevel::Critical,
        75..=89 => MetricLevel::Warning,
        _ => MetricLevel::Normal,
    };

    MetricVisual {
        active_segments,
        level,
        label: format!("{percent}%"),
    }
}

fn percent_label(percent: Option<u8>) -> String {
    percent.map_or_else(|| "--".to_string(), |value| value.to_string())
}

fn set_enabled_class(widget: &impl IsA<gtk::Widget>, enabled: bool) {
    if enabled {
        widget.add_css_class("enabled");
    } else {
        widget.remove_css_class("enabled");
    }
}

#[cfg(test)]
mod tests {
    use gtk4::glib;

    use crate::{
        AudioOutputState, AudioState, BarSnapshot, BluetoothDeviceOperation, BluetoothDeviceState,
        BluetoothState, BrightnessState, CalendarAgendaEvent, ConnectivityState,
        KeyboardLayoutOption, KeyboardLayoutState, MediaState, NetworkState, PlaybackStatus,
        PowerProfile, PowerState, ResourceState, TimerState,
    };

    use super::{
        ControlCenterErrors, ControlCenterFocus, ControlCenterMotionEvent,
        ControlCenterMotionPhase, MetricLevel, SliderDebounce, TimerActionState, TimerControlSpec,
        build_control_center_spec, control_center_origin, events_for_local_day, focus_from_origin,
        format_network_rate, metric_visual, quick_grid_placements, timer_action_state,
        volume_icon_name,
    };

    #[test]
    fn control_center_motion_reopens_during_exit() {
        let phase = ControlCenterMotionPhase::Visible
            .transition(ControlCenterMotionEvent::Dismiss)
            .transition(ControlCenterMotionEvent::Present);

        assert_eq!(phase, ControlCenterMotionPhase::Entering);
        assert!(phase.is_presented());
    }

    #[test]
    fn control_center_motion_only_completes_matching_phases() {
        assert_eq!(
            ControlCenterMotionPhase::Entering.transition(ControlCenterMotionEvent::Entered),
            ControlCenterMotionPhase::Visible
        );
        assert_eq!(
            ControlCenterMotionPhase::Exiting.transition(ControlCenterMotionEvent::Exited),
            ControlCenterMotionPhase::Hidden
        );
        assert_eq!(
            ControlCenterMotionPhase::Visible.transition(ControlCenterMotionEvent::Exited),
            ControlCenterMotionPhase::Visible
        );
    }

    #[test]
    fn volume_icons_follow_mute_and_level_boundaries() {
        assert_eq!(
            volume_icon_name(true, Some(80)),
            "audio-volume-muted-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(0)),
            "audio-volume-muted-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(1)),
            "audio-volume-low-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(33)),
            "audio-volume-low-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(34)),
            "audio-volume-medium-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(66)),
            "audio-volume-medium-symbolic"
        );
        assert_eq!(
            volume_icon_name(false, Some(67)),
            "audio-volume-high-symbolic"
        );
    }

    #[test]
    fn network_rates_use_compact_binary_units() {
        assert_eq!(format_network_rate(None), "--");
        assert_eq!(format_network_rate(Some(512)), "512 B/s");
        assert_eq!(format_network_rate(Some(1536)), "1.5 KB/s");
        assert_eq!(format_network_rate(Some(12 * 1024)), "12 KB/s");
        assert_eq!(format_network_rate(Some(3 * 1024 * 1024)), "3.0 MB/s");
    }

    #[test]
    fn selected_day_includes_overlapping_events_and_sorts_all_day_first() {
        let day = glib::DateTime::from_local(2027, 1, 20, 0, 0, 0.0).unwrap();
        let previous = day.add_days(-1).unwrap();
        let next = day.add_days(1).unwrap();
        let events = vec![
            CalendarAgendaEvent {
                id: "meeting".to_string(),
                title: "Meeting".to_string(),
                location: None,
                calendar: Some("Work".to_string()),
                start_epoch: day.to_unix() + 10 * 60 * 60,
                end_epoch: day.to_unix() + 11 * 60 * 60,
                all_day: false,
            },
            CalendarAgendaEvent {
                id: "conference".to_string(),
                title: "Conference".to_string(),
                location: None,
                calendar: None,
                start_epoch: previous.to_unix(),
                end_epoch: next.to_unix(),
                all_day: true,
            },
            CalendarAgendaEvent {
                id: "tomorrow".to_string(),
                title: "Tomorrow".to_string(),
                location: None,
                calendar: None,
                start_epoch: next.to_unix() + 60,
                end_epoch: next.to_unix() + 120,
                all_day: false,
            },
        ];

        let selected = events_for_local_day(&events, 2027, 1, 20);
        assert_eq!(
            selected
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["conference", "meeting"]
        );
    }

    #[test]
    fn overview_spec_combines_controls_metrics_and_optional_content() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: None,
            label: Some("Home".to_string()),
            wifi_available: true,
            ethernet_available: true,
            wifi_enabled: Some(true),
            interface: Some("wlan0".to_string()),
            download_bytes_per_second: Some(1536),
            upload_bytes_per_second: Some(512),
            download_history: vec![0, 512, 1536],
            upload_history: vec![0, 128, 512],
        };
        snapshot.system.bluetooth = BluetoothState {
            available: true,
            powered: true,
            connected_device: Some("Headphones".to_string()),
            audio_device: Some("Headphones".to_string()),
            ..BluetoothState::default()
        };
        snapshot.system.audio = AudioState {
            volume_percent: Some(49),
            muted: false,
            outputs: vec![AudioOutputState {
                name: "bluez_output.headphones".to_string(),
                description: "Robin's Headphones".to_string(),
                alias: Some("Headphones".to_string()),
                port_description: None,
                port_type: Some("Headphones".to_string()),
                bus: Some("bluetooth".to_string()),
                is_default: true,
            }],
        };
        snapshot.system.brightness = BrightnessState {
            device: Some("intel_backlight".to_string()),
            percent: Some(67),
        };
        snapshot.system.resources = ResourceState {
            cpu_percent: Some(14),
            memory_percent: Some(32),
        };
        snapshot.system.power = PowerState {
            battery_present: true,
            battery_percent: Some(82),
            charging: true,
            profile: PowerProfile::Balanced,
            changed_at: 0,
        };
        snapshot.system.keyboard_layout = KeyboardLayoutState {
            current_index: Some(0),
            current_name: Some("English (US)".to_string()),
            layouts: vec![
                KeyboardLayoutOption {
                    index: 0,
                    name: "English (US)".to_string(),
                    layout: Some("us".to_string()),
                    variant: None,
                },
                KeyboardLayoutOption {
                    index: 1,
                    name: "English (Dvorak)".to_string(),
                    layout: Some("us".to_string()),
                    variant: Some("dvorak".to_string()),
                },
                KeyboardLayoutOption {
                    index: 2,
                    name: "German (KOY)".to_string(),
                    layout: Some("de".to_string()),
                    variant: Some("koy".to_string()),
                },
            ],
        };
        snapshot.system.media = Some(MediaState {
            player: "spotify".to_string(),
            status: PlaybackStatus::Playing,
            title: Some("Says".to_string()),
            artist: Some("Nils Frahm".to_string()),
            art_url: Some("https://example.test/says.jpg".to_string()),
            changed_at: 0,
        });
        snapshot.system.timers = vec![TimerState {
            id: "timer-1".to_string(),
            label: "Tea".to_string(),
            remaining_seconds: 125,
            target_epoch: Some(125),
            completed: false,
            changed_at: 0,
        }];

        let spec = build_control_center_spec(&snapshot);

        assert_eq!(spec.network.detail, "Home");
        assert!(spec.network.enabled);
        assert_eq!(spec.network_traffic.interface.as_deref(), Some("wlan0"));
        assert_eq!(spec.network_traffic.download_history, vec![0, 512, 1536]);
        assert_eq!(spec.audio.percent, Some(49));
        assert_eq!(spec.audio.detail, "Headphones");
        assert_eq!(spec.audio.outputs[0].icon_name, "audio-headphones-symbolic");
        assert!(spec.audio.outputs[0].selected);
        assert_eq!(spec.brightness.as_ref().unwrap().device, "intel_backlight");
        assert_eq!(spec.cpu_percent, Some(14));
        assert_eq!(spec.memory_percent, Some(32));
        assert_eq!(spec.keyboard.summary, "US");
        assert_eq!(spec.keyboard.current, "US — Standard");
        assert_eq!(spec.keyboard.layouts[1].detail, "Dvorak");
        assert_eq!(spec.keyboard.layouts[2].title, "DE");
        assert_eq!(spec.keyboard.layouts[2].detail, "KOY");
        assert_eq!(spec.battery_percent, Some(82));
        assert!(spec.battery_present);
        assert!(spec.charging);
        assert_eq!(spec.media.as_ref().unwrap().title, "Says");
        assert_eq!(spec.media.as_ref().unwrap().player, "spotify");
        assert_eq!(spec.timers[0].remaining_seconds, 125);
        assert!(!spec.timers[0].paused);
    }

    #[test]
    fn overview_hides_brightness_when_no_real_backlight_is_available() {
        let spec = build_control_center_spec(&BarSnapshot::default());

        assert_eq!(spec.brightness, None);
    }

    #[test]
    fn wifi_tile_describes_wired_connectivity_without_using_connection_name() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: Some("network-wired-symbolic".to_string()),
            label: Some("Ethernet connection 1".to_string()),
            wifi_available: true,
            ethernet_available: true,
            wifi_enabled: Some(true),
            ..NetworkState::default()
        };

        let spec = build_control_center_spec(&snapshot);

        assert_eq!(spec.network.detail, "On · Ethernet active");
    }

    #[test]
    fn desktop_spec_uses_ethernet_and_omits_laptop_only_hardware() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: Some("network-wired-symbolic".to_string()),
            label: Some("Ethernet".to_string()),
            wifi_available: false,
            ethernet_available: true,
            wifi_enabled: None,
            ..NetworkState::default()
        };
        snapshot.system.bluetooth = BluetoothState {
            available: true,
            powered: true,
            ..BluetoothState::default()
        };
        snapshot.system.power.profile = PowerProfile::Performance;

        let spec = build_control_center_spec(&snapshot);

        assert_eq!(spec.network.label, "Ethernet");
        assert_eq!(spec.network.icon_name, "network-wired-symbolic");
        assert_eq!(spec.network.detail, "Ethernet");
        assert!(spec.network.available);
        assert!(spec.network.enabled);
        assert!(!spec.network.toggle_available);
        assert!(spec.bluetooth.available);
        assert!(!spec.battery_present);
        assert_eq!(spec.brightness, None);
        assert_eq!(spec.power.detail, "Performance");
    }

    #[test]
    fn bluetooth_manager_spec_preserves_device_sections_and_progress() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.bluetooth = BluetoothState {
            available: true,
            powered: true,
            discovering: true,
            devices: vec![
                BluetoothDeviceState {
                    address: "AA:00:00:00:00:01".to_string(),
                    name: "Headphones".to_string(),
                    icon_name: "audio-headphones-symbolic".to_string(),
                    paired: true,
                    trusted: true,
                    connected: true,
                    audio_capable: true,
                    battery_percent: Some(73),
                    ..BluetoothDeviceState::default()
                },
                BluetoothDeviceState {
                    address: "AA:00:00:00:00:02".to_string(),
                    name: "Controller".to_string(),
                    icon_name: "input-gaming-symbolic".to_string(),
                    paired: true,
                    operation: Some(BluetoothDeviceOperation::Connecting),
                    ..BluetoothDeviceState::default()
                },
            ],
            ..BluetoothState::default()
        };

        let spec = build_control_center_spec(&snapshot).bluetooth_manager;

        assert!(spec.discovering);
        assert_eq!(spec.connected_count, 1);
        assert_eq!(spec.devices[0].detail, "Connected · 73% battery");
        assert_eq!(spec.devices[1].detail, "Connecting…");
    }

    #[test]
    fn quick_grid_spans_the_final_tile_when_optional_hardware_is_absent() {
        assert_eq!(quick_grid_placements(0), vec![]);
        assert_eq!(quick_grid_placements(1), vec![(0, 0, 2)]);
        assert_eq!(
            quick_grid_placements(3),
            vec![(0, 0, 1), (1, 0, 1), (0, 1, 2)]
        );
        assert_eq!(
            quick_grid_placements(4),
            vec![(0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)]
        );
    }

    #[test]
    fn action_origins_round_trip_every_overview_and_detail_page() {
        for focus in ControlCenterFocus::ALL {
            let origin = control_center_origin(focus, "open");
            assert_eq!(focus_from_origin(&origin), Some(focus));
        }
        assert_eq!(focus_from_origin("window-popover:DP-5:42"), None);
    }

    #[test]
    fn bluetooth_has_independent_action_and_error_routing() {
        let origin = control_center_origin(ControlCenterFocus::Bluetooth, "toggle");
        assert_eq!(
            focus_from_origin(&origin),
            Some(ControlCenterFocus::Bluetooth)
        );

        let mut errors = ControlCenterErrors::default();
        errors.record_failure(ControlCenterFocus::Bluetooth, "bluetoothctl failed");
        assert_eq!(
            errors.message(ControlCenterFocus::Bluetooth),
            Some("bluetoothctl failed")
        );
        assert_eq!(errors.message(ControlCenterFocus::Audio), None);
    }

    #[test]
    fn slider_debounce_keeps_only_the_latest_clamped_value() {
        let mut debounce = SliderDebounce::default();
        debounce.schedule(20);
        debounce.schedule(140);

        assert_eq!(debounce.take(), Some(100));
        assert_eq!(debounce.take(), None);
    }

    #[test]
    fn metric_gauge_rounds_up_segments_and_applies_pressure_thresholds() {
        let cases = [
            (0, 0, MetricLevel::Normal),
            (1, 1, MetricLevel::Normal),
            (74, 8, MetricLevel::Normal),
            (75, 8, MetricLevel::Warning),
            (89, 9, MetricLevel::Warning),
            (90, 9, MetricLevel::Critical),
            (100, 10, MetricLevel::Critical),
            (140, 10, MetricLevel::Critical),
        ];

        for (percent, active_segments, level) in cases {
            let visual = metric_visual(Some(percent));
            assert_eq!(visual.active_segments, active_segments);
            assert_eq!(visual.level, level);
            assert_eq!(visual.label, format!("{}%", percent.min(100)));
        }
    }

    #[test]
    fn unavailable_metric_gauge_has_no_active_segments() {
        let visual = metric_visual(None);

        assert_eq!(visual.active_segments, 0);
        assert_eq!(visual.level, MetricLevel::Unavailable);
        assert_eq!(visual.label, "Unavailable");
    }

    #[test]
    fn countdown_ticks_preserve_the_interactive_timer_action_state() {
        let mut timer = TimerControlSpec {
            id: "timer-1".to_string(),
            label: "Tea".to_string(),
            remaining_seconds: 120,
            completed: false,
            paused: false,
        };
        let initial = timer_action_state(&timer);
        assert_eq!(initial, TimerActionState::Pause);

        timer.remaining_seconds = 119;
        assert_eq!(timer_action_state(&timer), initial);

        timer.paused = true;
        assert_eq!(timer_action_state(&timer), TimerActionState::Resume);
    }

    #[test]
    fn action_errors_are_retained_per_section_until_retry_or_close() {
        let mut errors = ControlCenterErrors::default();
        errors.record_failure(ControlCenterFocus::Network, "nmcli failed");
        errors.record_failure(ControlCenterFocus::Audio, "wpctl failed");

        errors.retry(ControlCenterFocus::Audio);
        assert_eq!(
            errors.message(ControlCenterFocus::Network),
            Some("nmcli failed")
        );
        assert_eq!(errors.message(ControlCenterFocus::Audio), None);

        errors.close();
        assert_eq!(errors.message(ControlCenterFocus::Network), None);
    }
}
