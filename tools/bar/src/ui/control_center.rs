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
    ActionCompletion, ActionIntent, ActionRequest, ActionResult, BarSnapshot, ConnectivityState,
    Direction, MediaControlAction, PlaybackStatus, PowerProfile,
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
            Self::Bluetooth => "Connected devices",
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickControlSpec {
    pub icon_name: String,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
    pub available: bool,
    pub toggle_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioControlSpec {
    pub percent: Option<u8>,
    pub muted: bool,
    pub detail: String,
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
    pub bluetooth: QuickControlSpec,
    pub audio: AudioControlSpec,
    pub power: QuickControlSpec,
    pub brightness: Option<BrightnessControlSpec>,
    pub keyboard: String,
    pub cpu_percent: Option<u8>,
    pub memory_percent: Option<u8>,
    pub battery_present: bool,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub clock: String,
    pub calendar: Option<CalendarControlSpec>,
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
    let audio_detail = system
        .bluetooth
        .audio_device
        .clone()
        .unwrap_or_else(|| "Default output".to_string());
    let media = system.media.as_ref().map(|media| MediaControlSpec {
        player: media.player.clone(),
        title: media.title.clone().unwrap_or_else(|| media.player.clone()),
        artist: media.artist.clone().unwrap_or_else(|| media.player.clone()),
        playing: media.status == PlaybackStatus::Playing,
        art_url: media.art_url.clone(),
    });

    ControlCenterSpec {
        network,
        bluetooth: QuickControlSpec {
            icon_name: "bluetooth-symbolic".to_string(),
            label: "Bluetooth".to_string(),
            detail: bluetooth_detail,
            enabled: system.bluetooth.powered,
            available: system.bluetooth.available,
            toggle_available: system.bluetooth.available,
        },
        audio: AudioControlSpec {
            percent: system.audio.volume_percent.map(|value| value.min(100)),
            muted: system.audio.muted,
            detail: audio_detail,
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
        keyboard: short_layout_label(system.keyboard_layout.as_deref()),
        cpu_percent: system.resources.cpu_percent,
        memory_percent: system.resources.memory_percent,
        battery_present: system.power.battery_present,
        battery_percent: system.power.battery_percent,
        charging: system.power.charging,
        clock: system.clock.label.clone(),
        calendar: system.calendar.as_ref().map(|event| CalendarControlSpec {
            title: event.title.clone(),
            location: event.location.clone(),
        }),
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

fn short_layout_label(layout: Option<&str>) -> String {
    let Some(layout) = layout else {
        return "--".to_string();
    };
    let lower = layout.to_ascii_lowercase();
    if lower.contains("dvorak") {
        "DV".to_string()
    } else if lower.contains("us") {
        "US".to_string()
    } else {
        layout.chars().take(2).collect::<String>().to_uppercase()
    }
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
    footer: gtk::Button,
    error_slot: gtk::Box,
    error_label: gtk::Label,
    errors: Rc<RefCell<ControlCenterErrors>>,
    pending: Rc<RefCell<BTreeMap<ControlCenterFocus, usize>>>,
}

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
        self.footer
            .set_label(&format!("Open {} in Luma", page.title()));
        self.render_error();
    }

    fn render_error(&self) {
        if let Some(message) = self.errors.borrow().message(self.page.get()) {
            self.error_label.set_label(message);
            self.error_slot.remove_css_class("is-pending");
            self.error_slot.add_css_class("has-error");
        } else if self
            .pending
            .borrow()
            .get(&self.page.get())
            .is_some_and(|count| *count > 0)
        {
            self.error_label.set_label("Applying change…");
            self.error_slot.remove_css_class("has-error");
            self.error_slot.add_css_class("is-pending");
        } else {
            self.error_label.set_label("");
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

const METRIC_SEGMENTS: usize = 10;
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
    network_page: ConnectivityPage,
    bluetooth_page: ConnectivityPage,
    audio_state: gtk::Label,
    audio_detail: gtk::Label,
    battery_hero: gtk::Box,
    battery_state: gtk::Label,
    battery_detail: gtk::Label,
    power_profile: gtk::Label,
    keyboard_state: gtk::Label,
    cpu_gauge: MetricGauge,
    memory_gauge: MetricGauge,
    calendar_title: gtk::Label,
    calendar_detail: gtk::Label,
    timer_list: gtk::Box,
    timer_empty: gtk::Label,
    timer_widgets: RefCell<TimerWidgets>,
    clock_label: gtk::Label,
    errors: Rc<RefCell<ControlCenterErrors>>,
    suppress_controls: Rc<Cell<bool>>,
    timer_sender: Sender<ActionRequest>,
}

impl ControlCenterView {
    pub fn new(
        application: &gtk::Application,
        monitor: &gtk::gdk::Monitor,
        top_margin: i32,
        right_margin: i32,
        spec: &ControlCenterSpec,
        action_sender: Sender<ActionRequest>,
    ) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("cockpit-quick-settings")
            .build();
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_default_size(512, 680);
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
        root.set_size_request(480, 648);

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
        stack.set_size_request(-1, 488);
        stack.set_transition_duration(180);
        stack.set_hhomogeneous(true);
        stack.set_vhomogeneous(true);

        let errors = Rc::new(RefCell::new(ControlCenterErrors::default()));
        let error_slot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        error_slot.add_css_class("control-error-slot");
        error_slot.set_size_request(-1, 32);
        let error_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
        let error_label = gtk::Label::new(None);
        error_label.set_xalign(0.0);
        error_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        error_label.set_hexpand(true);
        error_slot.append(&error_icon);
        error_slot.append(&error_label);

        let footer = gtk::Button::with_label("Open Quick Settings in Luma");
        footer.add_css_class("control-footer");
        footer.set_has_frame(false);

        let navigation = NavigationUi {
            page: Rc::new(Cell::new(ControlCenterFocus::Overview)),
            stack: stack.clone(),
            back_button: back_button.clone(),
            title,
            subtitle,
            footer: footer.clone(),
            error_slot: error_slot.clone(),
            error_label,
            errors: errors.clone(),
            pending: Rc::new(RefCell::new(BTreeMap::new())),
        };

        let current = Rc::new(RefCell::new(spec.clone()));
        let suppress_controls = Rc::new(Cell::new(false));
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
        let (keyboard_summary_button, keyboard_summary) = summary_tile("Keyboard");
        let (resources_summary_button, resources_summary) = summary_tile("CPU / RAM");
        let (battery_summary_button, battery_summary) = summary_tile("Battery");
        let (time_summary_button, time_summary) = summary_tile("Focus");
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
        overview.append(&summaries);
        stack.add_named(&overview, Some(ControlCenterFocus::Overview.as_str()));

        let network_page = connectivity_page("network-wireless-symbolic", "Wi-Fi");
        stack.add_named(
            &network_page.root,
            Some(ControlCenterFocus::Network.as_str()),
        );

        let bluetooth_page = connectivity_page("bluetooth-symbolic", "Bluetooth");
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
        let (keyboard_hero, keyboard_state, _) =
            detail_hero("input-keyboard-symbolic", "Current layout");
        keyboard_page.append(&keyboard_hero);
        let keyboard_button = gtk::Button::with_label("Switch keyboard layout");
        keyboard_button.add_css_class("primary-action");
        keyboard_page.append(&keyboard_button);
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

        let clock_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
        clock_page.add_css_class("control-page");
        let calendar_card = gtk::Box::new(gtk::Orientation::Vertical, 3);
        calendar_card.add_css_class("detail-card");
        let calendar_eyebrow = gtk::Label::new(Some("NEXT EVENT"));
        calendar_eyebrow.add_css_class("detail-eyebrow");
        calendar_eyebrow.set_xalign(0.0);
        let calendar_title = gtk::Label::new(None);
        calendar_title.add_css_class("detail-card-title");
        calendar_title.set_xalign(0.0);
        calendar_title.set_max_width_chars(40);
        calendar_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let calendar_detail = gtk::Label::new(None);
        calendar_detail.add_css_class("supporting-text");
        calendar_detail.set_xalign(0.0);
        calendar_detail.set_max_width_chars(40);
        calendar_detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        calendar_card.append(&calendar_eyebrow);
        calendar_card.append(&calendar_title);
        calendar_card.append(&calendar_detail);
        clock_page.append(&calendar_card);
        let timers_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let timers_title = gtk::Label::new(Some("Timers"));
        timers_title.add_css_class("section-title");
        timers_title.set_xalign(0.0);
        timers_title.set_hexpand(true);
        let quick_timer = gtk::Button::with_label("+ 5 min");
        quick_timer.add_css_class("compact-action");
        timers_heading.append(&timers_title);
        timers_heading.append(&quick_timer);
        clock_page.append(&timers_heading);
        let timer_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let timer_empty = gtk::Label::new(Some("No active timers"));
        timer_empty.add_css_class("empty-state");
        timer_empty.set_xalign(0.0);
        timer_list.append(&timer_empty);
        let timer_scroll = gtk::ScrolledWindow::new();
        timer_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        timer_scroll.set_min_content_height(150);
        timer_scroll.set_child(Some(&timer_list));
        clock_page.append(&timer_scroll);
        stack.add_named(&clock_page, Some(ControlCenterFocus::Clock.as_str()));

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
        footer.connect_clicked(move |_| {
            let focus = nav_for_footer.page.get();
            let handle = ActionHandle {
                sender: sender_for_footer.clone(),
                errors: nav_for_footer.errors.clone(),
                navigation: nav_for_footer.clone(),
                focus,
            };
            handle.send(
                "open-luma",
                ActionIntent::OpenContextQuery {
                    query: focus.luma_query().to_string(),
                },
            );
            motion_for_footer.dismiss();
        });

        connect_toggle_controls(
            [&network_tile.toggle, &network_page.toggle],
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
            |spec, active| ((!spec.audio.muted) != active).then_some(ActionIntent::ToggleMute),
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

        let keyboard_action = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Keyboard,
        };
        keyboard_button.connect_clicked(move |_| {
            keyboard_action.send("cycle-layout", ActionIntent::ToggleKeyboardLayout);
        });

        let quick_timer_action = ActionHandle {
            sender: action_sender.clone(),
            errors: errors.clone(),
            navigation: navigation.clone(),
            focus: ControlCenterFocus::Clock,
        };
        quick_timer.connect_clicked(move |_| {
            quick_timer_action.send(
                "start-timer",
                ActionIntent::StartTimer {
                    label: "Quick timer".to_string(),
                    duration_seconds: 300,
                },
            );
        });

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
        window.connect_visible_notify(move |window| {
            if !window.is_visible() {
                errors_for_close.borrow_mut().close();
                nav_for_close.pending.borrow_mut().clear();
                nav_for_close.render_error();
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
            cpu_gauge,
            memory_gauge,
            calendar_title,
            calendar_detail,
            timer_list,
            timer_empty,
            timer_widgets: RefCell::new(BTreeMap::new()),
            clock_label: clock,
            errors,
            suppress_controls,
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
    }

    pub fn dismiss(&self) {
        self.motion.dismiss();
    }

    pub fn is_visible(&self) -> bool {
        self.motion.is_presented()
    }

    pub fn destroy(&self) {
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
        self.audio_tile.detail.set_label(&format!(
            "{} · {}%",
            if spec.audio.muted {
                "Muted"
            } else {
                &spec.audio.detail
            },
            spec.audio.percent.unwrap_or_default()
        ));
        set_enabled_class(&self.audio_tile.root, !spec.audio.muted);
        self.audio_tile.toggle.set_active(!spec.audio.muted);
        update_action_tile(&self.power_tile, &spec.power);

        let volume = spec.audio.percent.unwrap_or_default();
        for scale in &self.volume_scales {
            scale.set_value(f64::from(volume));
        }
        for label in &self.volume_values {
            label.set_label(&format!("{volume}%"));
        }
        for button in &self.volume_buttons {
            button.set_icon_name(volume_icon_name(spec.audio.muted, spec.audio.percent));
            button.set_tooltip_text(Some(if spec.audio.muted { "Unmute" } else { "Mute" }));
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

        self.keyboard_summary.set_label(&spec.keyboard);
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

        update_connectivity_page(&self.network_page, &spec.network);
        update_connectivity_page(&self.bluetooth_page, &spec.bluetooth);
        self.audio_state.set_label(if spec.audio.muted {
            "Muted"
        } else {
            "Sound on"
        });
        self.audio_detail.set_label(&spec.audio.detail);

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
        self.keyboard_state.set_label(&spec.keyboard);
        update_metric(&self.cpu_gauge, spec.cpu_percent);
        update_metric(&self.memory_gauge, spec.memory_percent);

        if let Some(calendar) = spec.calendar.as_ref() {
            self.calendar_title.set_label(&calendar.title);
            self.calendar_detail
                .set_label(calendar.location.as_deref().unwrap_or("Upcoming"));
        } else {
            self.calendar_title.set_label("Nothing upcoming");
            self.calendar_detail.set_label("Calendar is clear");
        }
        reconcile_timers(
            &self.timer_list,
            &self.timer_empty,
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
                self.errors.borrow_mut().record_failure(focus, detail)
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

fn summary_tile(title: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("summary-tile");
    button.set_has_frame(false);
    button.set_hexpand(true);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let value = gtk::Label::new(None);
    value.set_max_width_chars(12);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("supporting-text");
    column.append(&value);
    column.append(&title);
    button.set_child(Some(&column));
    (button, value)
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
    use crate::{
        AudioState, BarSnapshot, BluetoothState, BrightnessState, ConnectivityState, MediaState,
        NetworkState, PlaybackStatus, PowerProfile, PowerState, ResourceState, TimerState,
    };

    use super::{
        ControlCenterErrors, ControlCenterFocus, ControlCenterMotionEvent,
        ControlCenterMotionPhase, MetricLevel, SliderDebounce, TimerActionState, TimerControlSpec,
        build_control_center_spec, control_center_origin, focus_from_origin, metric_visual,
        quick_grid_placements, timer_action_state, volume_icon_name,
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
    fn overview_spec_combines_controls_metrics_and_optional_content() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: None,
            label: Some("Home".to_string()),
            wifi_available: true,
            ethernet_available: true,
            wifi_enabled: Some(true),
        };
        snapshot.system.bluetooth = BluetoothState {
            available: true,
            powered: true,
            connected_device: Some("Headphones".to_string()),
            audio_device: Some("Headphones".to_string()),
        };
        snapshot.system.audio = AudioState {
            volume_percent: Some(49),
            muted: false,
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
        snapshot.system.keyboard_layout = Some("English (US)".to_string());
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
        assert_eq!(spec.audio.percent, Some(49));
        assert_eq!(spec.brightness.as_ref().unwrap().device, "intel_backlight");
        assert_eq!(spec.cpu_percent, Some(14));
        assert_eq!(spec.memory_percent, Some(32));
        assert_eq!(spec.keyboard, "US");
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
