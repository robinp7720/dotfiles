use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;

use crate::{
    ActionCompletion, ActionIntent, ActionRequest, ActionResult, BarSnapshot, Direction,
    MediaControlAction, PlaybackStatus, PowerProfile,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlCenterFocus {
    Keyboard,
    Resources,
    Network,
    Audio,
    Power,
    Clock,
}

impl ControlCenterFocus {
    pub const ALL: [Self; 6] = [
        Self::Keyboard,
        Self::Resources,
        Self::Network,
        Self::Audio,
        Self::Power,
        Self::Clock,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Resources => "resources",
            Self::Network => "network",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Clock => "clock",
        }
    }

    pub fn luma_query(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Resources => "system",
            Self::Network => "network",
            Self::Audio => "audio",
            Self::Power => "power",
            Self::Clock => "calendar",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickControlSpec {
    pub label: String,
    pub detail: String,
    pub enabled: bool,
    pub available: bool,
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
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlCenterSpec {
    pub network: QuickControlSpec,
    pub bluetooth: QuickControlSpec,
    pub audio: AudioControlSpec,
    pub power: QuickControlSpec,
    pub brightness: Option<BrightnessControlSpec>,
    pub keyboard: String,
    pub resources: String,
    pub battery: String,
    pub clock: String,
    pub calendar: Option<String>,
    pub timer: Option<String>,
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
    let network_available = system.network.wifi_enabled.is_some();
    let network_enabled = system.network.wifi_enabled.unwrap_or(false);
    let network_detail = system.network.label.clone().unwrap_or_else(|| {
        if network_enabled {
            "Not connected"
        } else {
            "Off"
        }
        .to_string()
    });
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
    let power_detail = power_profile_label(&system.power.profile).to_string();
    let media = system.media.as_ref().map(|media| MediaControlSpec {
        title: media.title.clone().unwrap_or_else(|| media.player.clone()),
        artist: media.artist.clone().unwrap_or_else(|| media.player.clone()),
        playing: media.status == PlaybackStatus::Playing,
    });

    ControlCenterSpec {
        network: QuickControlSpec {
            label: "Wi-Fi".to_string(),
            detail: network_detail,
            enabled: network_enabled,
            available: network_available,
        },
        bluetooth: QuickControlSpec {
            label: "Bluetooth".to_string(),
            detail: bluetooth_detail,
            enabled: system.bluetooth.powered,
            available: true,
        },
        audio: AudioControlSpec {
            percent: system.audio.volume_percent.map(|value| value.min(100)),
            muted: system.audio.muted,
            detail: audio_detail,
        },
        power: QuickControlSpec {
            label: "Power".to_string(),
            detail: power_detail,
            enabled: system.power.profile != PowerProfile::Balanced,
            available: true,
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
        resources: format!(
            "{} / {}",
            percent_label(system.resources.cpu_percent),
            percent_label(system.resources.memory_percent)
        ),
        battery: system
            .power
            .battery_percent
            .map(|percent| format!("{percent}%"))
            .unwrap_or_else(|| "--".to_string()),
        clock: system.clock.label.clone(),
        calendar: system.calendar.as_ref().map(|event| event.title.clone()),
        timer: system.timers.first().map(|timer| {
            if timer.completed {
                format!("{} complete", timer.label)
            } else {
                format!(
                    "{} · {}m",
                    timer.label,
                    timer.remaining_seconds.div_ceil(60)
                )
            }
        }),
        media,
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

fn percent_label(percent: Option<u8>) -> String {
    percent.map_or_else(|| "--".to_string(), |value| value.to_string())
}

fn power_profile_label(profile: &PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "Performance",
        PowerProfile::Balanced => "Balanced",
        PowerProfile::PowerSaver => "Power saver",
    }
}

pub struct ControlCenterView {
    popover: gtk::Popover,
    current: Rc<RefCell<ControlCenterSpec>>,
    focus: Rc<Cell<ControlCenterFocus>>,
    focus_widgets: BTreeMap<ControlCenterFocus, gtk::Widget>,
    network_button: gtk::Button,
    network_detail: gtk::Label,
    bluetooth_button: gtk::Button,
    bluetooth_detail: gtk::Label,
    audio_button: gtk::Button,
    audio_detail: gtk::Label,
    power_button: gtk::Button,
    power_detail: gtk::Label,
    volume_scale: gtk::Scale,
    volume_value: gtk::Label,
    brightness_row: gtk::Box,
    brightness_scale: gtk::Scale,
    brightness_value: gtk::Label,
    brightness_device: Rc<RefCell<Option<String>>>,
    media_row: gtk::Box,
    media_title: gtk::Label,
    media_artist: gtk::Label,
    play_button: gtk::Button,
    clock_label: gtk::Label,
    keyboard_label: gtk::Label,
    resources_label: gtk::Label,
    battery_label: gtk::Label,
    time_label: gtk::Label,
    error_label: gtk::Label,
    suppress_slider_actions: Rc<Cell<bool>>,
}

impl ControlCenterView {
    pub fn new(
        anchor: &impl IsA<gtk::Widget>,
        spec: &ControlCenterSpec,
        action_sender: Sender<ActionRequest>,
    ) -> Self {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.add_css_class("control-center");
        popover.set_parent(anchor);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("control-center-root");

        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let heading_text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let title = gtk::Label::new(Some("System"));
        title.add_css_class("control-center-title");
        title.set_xalign(0.0);
        let subtitle = gtk::Label::new(Some("Quick controls"));
        subtitle.add_css_class("supporting-text");
        subtitle.set_xalign(0.0);
        heading_text.append(&title);
        heading_text.append(&subtitle);
        heading_text.set_hexpand(true);
        let clock = gtk::Label::new(Some(&spec.clock));
        clock.add_css_class("control-center-clock");
        heading.append(&heading_text);
        heading.append(&clock);
        root.append(&heading);

        let current = Rc::new(RefCell::new(spec.clone()));
        let focus = Rc::new(Cell::new(ControlCenterFocus::Network));
        let error_label = gtk::Label::new(None);
        error_label.add_css_class("control-error");
        error_label.set_xalign(0.0);
        error_label.set_wrap(true);
        error_label.set_visible(false);

        let quick_grid = gtk::Grid::new();
        quick_grid.add_css_class("control-grid");
        quick_grid.set_row_spacing(8);
        quick_grid.set_column_spacing(8);
        quick_grid.set_column_homogeneous(true);

        let (network_button, network_detail) = quick_tile("network-wireless-symbolic", "Wi-Fi");
        let current_for_network = current.clone();
        let sender_for_network = action_sender.clone();
        let error_for_network = error_label.clone();
        network_button.connect_clicked(move |_| {
            let network = &current_for_network.borrow().network;
            if !network.available {
                return;
            }
            clear_error(&error_for_network);
            send_action(
                &sender_for_network,
                ControlCenterFocus::Network,
                "toggle-wifi",
                ActionIntent::SetWifiEnabled {
                    enabled: !network.enabled,
                },
            );
        });

        let (bluetooth_button, bluetooth_detail) = quick_tile("bluetooth-symbolic", "Bluetooth");
        let current_for_bluetooth = current.clone();
        let sender_for_bluetooth = action_sender.clone();
        let error_for_bluetooth = error_label.clone();
        bluetooth_button.connect_clicked(move |_| {
            let powered = current_for_bluetooth.borrow().bluetooth.enabled;
            clear_error(&error_for_bluetooth);
            send_action(
                &sender_for_bluetooth,
                ControlCenterFocus::Audio,
                "toggle-bluetooth",
                ActionIntent::SetBluetoothPowered { powered: !powered },
            );
        });

        let (audio_button, audio_detail) = quick_tile("audio-volume-high-symbolic", "Audio");
        let sender_for_mute = action_sender.clone();
        let error_for_mute = error_label.clone();
        audio_button.connect_clicked(move |_| {
            clear_error(&error_for_mute);
            send_action(
                &sender_for_mute,
                ControlCenterFocus::Audio,
                "toggle-mute",
                ActionIntent::ToggleMute,
            );
        });

        let (power_button, power_detail) = quick_tile("power-profile-balanced-symbolic", "Power");
        let sender_for_power = action_sender.clone();
        let error_for_power = error_label.clone();
        power_button.connect_clicked(move |_| {
            clear_error(&error_for_power);
            send_action(
                &sender_for_power,
                ControlCenterFocus::Power,
                "cycle-profile",
                ActionIntent::CyclePowerProfile {
                    direction: Direction::Next,
                },
            );
        });

        quick_grid.attach(&network_button, 0, 0, 1, 1);
        quick_grid.attach(&audio_button, 1, 0, 1, 1);
        quick_grid.attach(&bluetooth_button, 0, 1, 1, 1);
        quick_grid.attach(&power_button, 1, 1, 1, 1);
        root.append(&quick_grid);

        let suppress_slider_actions = Rc::new(Cell::new(false));
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        volume_row.add_css_class("control-slider-row");
        volume_row.append(&gtk::Image::from_icon_name("audio-volume-high-symbolic"));
        let volume_scale = control_scale();
        let volume_value = gtk::Label::new(None);
        volume_value.add_css_class("supporting-text");
        volume_row.append(&volume_scale);
        volume_row.append(&volume_value);
        root.append(&volume_row);
        install_percent_debounce(
            &volume_scale,
            suppress_slider_actions.clone(),
            action_sender.clone(),
            ControlCenterFocus::Audio,
            "set-volume",
            move |percent| ActionIntent::SetVolumePercent { percent },
        );

        let brightness_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        brightness_row.add_css_class("control-slider-row");
        brightness_row.append(&gtk::Image::from_icon_name("display-brightness-symbolic"));
        let brightness_scale = control_scale();
        let brightness_value = gtk::Label::new(None);
        brightness_value.add_css_class("supporting-text");
        brightness_row.append(&brightness_scale);
        brightness_row.append(&brightness_value);
        root.append(&brightness_row);
        let brightness_device = Rc::new(RefCell::new(
            spec.brightness.as_ref().map(|value| value.device.clone()),
        ));
        let device_for_slider = brightness_device.clone();
        install_percent_debounce(
            &brightness_scale,
            suppress_slider_actions.clone(),
            action_sender.clone(),
            ControlCenterFocus::Power,
            "set-brightness",
            move |percent| ActionIntent::SetBrightnessPercent {
                device: device_for_slider.borrow().clone().unwrap_or_default(),
                percent,
            },
        );

        let media_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        media_row.add_css_class("media-card");
        let media_text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        media_text.set_hexpand(true);
        let media_title = gtk::Label::new(None);
        media_title.set_xalign(0.0);
        let media_artist = gtk::Label::new(None);
        media_artist.add_css_class("supporting-text");
        media_artist.set_xalign(0.0);
        media_text.append(&media_title);
        media_text.append(&media_artist);
        media_row.append(&media_text);
        let previous_button = icon_button("media-skip-backward-symbolic", "Previous");
        let play_button = icon_button("media-playback-start-symbolic", "Play or pause");
        let next_button = icon_button("media-skip-forward-symbolic", "Next");
        connect_static_action(
            &previous_button,
            action_sender.clone(),
            error_label.clone(),
            ControlCenterFocus::Audio,
            "previous",
            ActionIntent::ControlMedia(MediaControlAction::Previous),
        );
        connect_static_action(
            &play_button,
            action_sender.clone(),
            error_label.clone(),
            ControlCenterFocus::Audio,
            "play-pause",
            ActionIntent::ControlMedia(MediaControlAction::PlayPause),
        );
        connect_static_action(
            &next_button,
            action_sender.clone(),
            error_label.clone(),
            ControlCenterFocus::Audio,
            "next",
            ActionIntent::ControlMedia(MediaControlAction::Next),
        );
        media_row.append(&previous_button);
        media_row.append(&play_button);
        media_row.append(&next_button);
        root.append(&media_row);

        let summaries = gtk::Grid::new();
        summaries.add_css_class("summary-grid");
        summaries.set_column_homogeneous(true);
        summaries.set_column_spacing(8);
        let (keyboard_button, keyboard_label) = summary_tile("Keyboard");
        let (resources_button, resources_label) = summary_tile("CPU / RAM");
        let (battery_button, battery_label) = summary_tile("Battery");
        let (time_button, time_label) = summary_tile("Time / timer");
        connect_static_action(
            &keyboard_button,
            action_sender.clone(),
            error_label.clone(),
            ControlCenterFocus::Keyboard,
            "cycle-layout",
            ActionIntent::ToggleKeyboardLayout,
        );
        connect_static_action(
            &time_button,
            action_sender.clone(),
            error_label.clone(),
            ControlCenterFocus::Clock,
            "start-timer",
            ActionIntent::StartTimer {
                label: "Quick timer".to_string(),
                duration_seconds: 300,
            },
        );
        summaries.attach(&keyboard_button, 0, 0, 1, 1);
        summaries.attach(&resources_button, 1, 0, 1, 1);
        summaries.attach(&battery_button, 2, 0, 1, 1);
        summaries.attach(&time_button, 3, 0, 1, 1);
        root.append(&summaries);
        root.append(&error_label);

        let footer = gtk::Button::with_label("Open in Luma");
        footer.add_css_class("control-footer");
        footer.set_has_frame(false);
        let focus_for_footer = focus.clone();
        let sender_for_footer = action_sender;
        let popover_for_footer = popover.clone();
        footer.connect_clicked(move |_| {
            let focus = focus_for_footer.get();
            send_action(
                &sender_for_footer,
                focus,
                "open-luma",
                ActionIntent::OpenContextQuery {
                    query: focus.luma_query().to_string(),
                },
            );
            popover_for_footer.popdown();
        });
        root.append(&footer);
        popover.set_child(Some(&root));

        let mut focus_widgets = BTreeMap::new();
        focus_widgets.insert(ControlCenterFocus::Keyboard, keyboard_button.upcast());
        focus_widgets.insert(ControlCenterFocus::Resources, resources_button.upcast());
        focus_widgets.insert(ControlCenterFocus::Network, network_button.clone().upcast());
        focus_widgets.insert(ControlCenterFocus::Audio, audio_button.clone().upcast());
        focus_widgets.insert(ControlCenterFocus::Power, power_button.clone().upcast());
        focus_widgets.insert(ControlCenterFocus::Clock, time_button.upcast());

        let view = Self {
            popover,
            current,
            focus,
            focus_widgets,
            network_button,
            network_detail,
            bluetooth_button,
            bluetooth_detail,
            audio_button,
            audio_detail,
            power_button,
            power_detail,
            volume_scale,
            volume_value,
            brightness_row,
            brightness_scale,
            brightness_value,
            brightness_device,
            media_row,
            media_title,
            media_artist,
            play_button,
            clock_label: clock,
            keyboard_label,
            resources_label,
            battery_label,
            time_label,
            error_label,
            suppress_slider_actions,
        };
        view.update(spec);
        view
    }

    pub fn popover(&self) -> &gtk::Popover {
        &self.popover
    }

    pub fn show(&self, focus: ControlCenterFocus) {
        self.focus(focus);
        self.popover.popup();
    }

    pub fn focus(&self, focus: ControlCenterFocus) {
        self.focus.set(focus);
        for (candidate, widget) in &self.focus_widgets {
            if *candidate == focus {
                widget.add_css_class("focused");
                widget.grab_focus();
            } else {
                widget.remove_css_class("focused");
            }
        }
        clear_error(&self.error_label);
    }

    pub fn update(&self, spec: &ControlCenterSpec) {
        *self.current.borrow_mut() = spec.clone();
        update_quick_tile(&self.network_button, &self.network_detail, &spec.network);
        update_quick_tile(
            &self.bluetooth_button,
            &self.bluetooth_detail,
            &spec.bluetooth,
        );
        self.audio_detail.set_label(&format!(
            "{} · {}%",
            if spec.audio.muted {
                "Muted"
            } else {
                &spec.audio.detail
            },
            spec.audio.percent.unwrap_or_default()
        ));
        set_enabled_class(&self.audio_button, !spec.audio.muted);
        update_quick_tile(&self.power_button, &self.power_detail, &spec.power);

        self.suppress_slider_actions.set(true);
        let volume = spec.audio.percent.unwrap_or_default();
        self.volume_scale.set_value(f64::from(volume));
        self.volume_value.set_label(&format!("{volume}%"));
        if let Some(brightness) = spec.brightness.as_ref() {
            *self.brightness_device.borrow_mut() = Some(brightness.device.clone());
            self.brightness_scale
                .set_value(f64::from(brightness.percent));
            self.brightness_value
                .set_label(&format!("{}%", brightness.percent));
            self.brightness_row.set_visible(true);
        } else {
            *self.brightness_device.borrow_mut() = None;
            self.brightness_row.set_visible(false);
        }
        self.suppress_slider_actions.set(false);

        if let Some(media) = spec.media.as_ref() {
            self.media_title.set_label(&media.title);
            self.media_artist.set_label(&media.artist);
            self.play_button.set_icon_name(if media.playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
            self.media_row.set_visible(true);
        } else {
            self.media_row.set_visible(false);
        }
        self.keyboard_label.set_label(&spec.keyboard);
        self.resources_label.set_label(&spec.resources);
        self.battery_label.set_label(&spec.battery);
        self.clock_label.set_label(&spec.clock);
        let time = spec
            .timer
            .as_ref()
            .or(spec.calendar.as_ref())
            .unwrap_or(&spec.clock);
        self.time_label.set_label(time);
    }

    pub fn handle_completion(&self, completion: &ActionCompletion) -> bool {
        if focus_from_origin(&completion.origin).is_none() {
            return false;
        }
        match &completion.result {
            ActionResult::Completed => clear_error(&self.error_label),
            ActionResult::Failed { detail, .. } => {
                self.error_label.set_label(detail);
                self.error_label.set_visible(true);
            }
        }
        true
    }
}

impl Drop for ControlCenterView {
    fn drop(&mut self) {
        self.popover.popdown();
        self.popover.unparent();
    }
}

fn quick_tile(icon_name: &str, title: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("quick-tile");
    button.set_has_frame(false);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&gtk::Image::from_icon_name(icon_name));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    let detail = gtk::Label::new(None);
    detail.add_css_class("supporting-text");
    detail.set_xalign(0.0);
    text.append(&title);
    text.append(&detail);
    text.set_hexpand(true);
    row.append(&text);
    button.set_child(Some(&row));
    (button, detail)
}

fn summary_tile(title: &str) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("summary-tile");
    button.set_has_frame(false);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let value = gtk::Label::new(None);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("supporting-text");
    column.append(&value);
    column.append(&title);
    button.set_child(Some(&column));
    (button, value)
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

fn install_percent_debounce<F>(
    scale: &gtk::Scale,
    suppress: Rc<Cell<bool>>,
    sender: Sender<ActionRequest>,
    focus: ControlCenterFocus,
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
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let percent = (scale.value().round() as u8).min(100);
        let sender = sender.clone();
        let pending_for_timeout = pending.clone();
        let intent = intent.clone();
        *pending.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(150),
            move || {
                send_action(&sender, focus, action, intent(percent));
                pending_for_timeout.borrow_mut().take();
            },
        ));
    });
}

fn connect_static_action(
    button: &gtk::Button,
    sender: Sender<ActionRequest>,
    error_label: gtk::Label,
    focus: ControlCenterFocus,
    action: &'static str,
    intent: ActionIntent,
) {
    button.connect_clicked(move |_| {
        clear_error(&error_label);
        send_action(&sender, focus, action, intent.clone());
    });
}

fn send_action(
    sender: &Sender<ActionRequest>,
    focus: ControlCenterFocus,
    action: &str,
    intent: ActionIntent,
) {
    let _ = sender.send(ActionRequest {
        origin: control_center_origin(focus, action),
        intent,
    });
}

fn update_quick_tile(button: &gtk::Button, detail: &gtk::Label, spec: &QuickControlSpec) {
    detail.set_label(&spec.detail);
    button.set_sensitive(spec.available);
    set_enabled_class(button, spec.enabled);
}

fn set_enabled_class(widget: &impl IsA<gtk::Widget>, enabled: bool) {
    if enabled {
        widget.add_css_class("enabled");
    } else {
        widget.remove_css_class("enabled");
    }
}

fn clear_error(label: &gtk::Label) {
    label.set_label("");
    label.set_visible(false);
}

#[cfg(test)]
mod tests {
    use crate::{
        AudioState, BarSnapshot, BluetoothState, BrightnessState, ConnectivityState, MediaState,
        NetworkState, PlaybackStatus, PowerProfile, PowerState, ResourceState,
    };

    use super::{
        ControlCenterFocus, SliderDebounce, build_control_center_spec, control_center_origin,
        focus_from_origin,
    };

    #[test]
    fn dashboard_spec_combines_quick_controls_summaries_and_optional_brightness() {
        let mut snapshot = BarSnapshot::default();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: None,
            label: Some("Home".to_string()),
            wifi_enabled: Some(true),
        };
        snapshot.system.bluetooth = BluetoothState {
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
            battery_percent: Some(82),
            charging: false,
            profile: PowerProfile::Balanced,
            changed_at: 0,
        };
        snapshot.system.keyboard_layout = Some("English (US)".to_string());
        snapshot.system.media = Some(MediaState {
            player: "spotify".to_string(),
            status: PlaybackStatus::Playing,
            title: Some("Says".to_string()),
            artist: Some("Nils Frahm".to_string()),
            changed_at: 0,
        });

        let spec = build_control_center_spec(&snapshot);

        assert_eq!(spec.network.detail, "Home");
        assert!(spec.network.enabled);
        assert_eq!(spec.audio.percent, Some(49));
        assert_eq!(spec.brightness.as_ref().unwrap().device, "intel_backlight");
        assert_eq!(spec.resources, "14 / 32");
        assert_eq!(spec.keyboard, "US");
        assert_eq!(spec.battery, "82%");
        assert_eq!(spec.media.as_ref().unwrap().title, "Says");
    }

    #[test]
    fn dashboard_hides_brightness_when_no_real_backlight_is_available() {
        let spec = build_control_center_spec(&BarSnapshot::default());

        assert_eq!(spec.brightness, None);
    }

    #[test]
    fn status_origins_round_trip_the_focused_dashboard_section() {
        for focus in ControlCenterFocus::ALL {
            let origin = control_center_origin(focus, "open");
            assert_eq!(focus_from_origin(&origin), Some(focus));
        }
        assert_eq!(focus_from_origin("window-popover:DP-5:42"), None);
    }

    #[test]
    fn slider_debounce_keeps_only_the_latest_clamped_value() {
        let mut debounce = SliderDebounce::default();
        debounce.schedule(20);
        debounce.schedule(140);

        assert_eq!(debounce.take(), Some(100));
        assert_eq!(debounce.take(), None);
    }
}
