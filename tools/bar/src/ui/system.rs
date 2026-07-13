use crate::{
    ActionIntent, ActionResult, AppConfig, AudioState, BarSnapshot, ConnectivityState, Direction,
    MediaControlAction, PlaybackStatus, PowerProfile, SourceHealth, SourceId, TimerState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemModuleId {
    Keyboard,
    Resources,
    Network,
    Audio,
    Power,
    Clock,
}

impl SystemModuleId {
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

    fn title(self) -> &'static str {
        match self {
            Self::Keyboard => "Keyboard",
            Self::Resources => "Resources",
            Self::Network => "Network",
            Self::Audio => "Audio",
            Self::Power => "Power",
            Self::Clock => "Clock",
        }
    }

    fn luma_query(self) -> &'static str {
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
pub struct SystemButtonSpec {
    pub id: SystemModuleId,
    pub icon_name: String,
    pub label: Option<String>,
    pub tooltip: String,
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemActionSpec {
    pub label: String,
    pub origin: String,
    pub intent: ActionIntent,
    pub closes_popover: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemPopoverSpec {
    pub id: SystemModuleId,
    pub title: String,
    pub lines: Vec<String>,
    pub controls: Vec<SystemActionSpec>,
    pub footer: SystemActionSpec,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemModuleSpec {
    pub button: SystemButtonSpec,
    pub popover: SystemPopoverSpec,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemCluster {
    modules: Vec<SystemModuleSpec>,
}

impl SystemCluster {
    pub fn modules(&self) -> &[SystemModuleSpec] {
        &self.modules
    }

    pub fn module(&self, id: SystemModuleId) -> &SystemButtonSpec {
        &self
            .modules
            .iter()
            .find(|module| module.button.id == id)
            .expect("system module")
            .button
    }

    pub fn popover(&self, id: SystemModuleId) -> &SystemPopoverSpec {
        &self
            .modules
            .iter()
            .find(|module| module.button.id == id)
            .expect("system popover")
            .popover
    }
}

pub fn build_system_cluster(snapshot: &BarSnapshot, config: &AppConfig) -> SystemCluster {
    let module_ids = [
        SystemModuleId::Keyboard,
        SystemModuleId::Resources,
        SystemModuleId::Network,
        SystemModuleId::Audio,
        SystemModuleId::Power,
        SystemModuleId::Clock,
    ];

    let modules = module_ids
        .into_iter()
        .map(|id| SystemModuleSpec {
            button: build_button_spec(id, snapshot, config),
            popover: build_popover_spec(id, snapshot, config, None),
        })
        .collect();

    SystemCluster { modules }
}

pub fn build_popover_spec(
    id: SystemModuleId,
    snapshot: &BarSnapshot,
    config: &AppConfig,
    result: Option<&ActionResult>,
) -> SystemPopoverSpec {
    let lines = match id {
        SystemModuleId::Keyboard => keyboard_lines(snapshot),
        SystemModuleId::Resources => resource_lines(snapshot),
        SystemModuleId::Network => network_lines(snapshot),
        SystemModuleId::Audio => audio_lines(snapshot),
        SystemModuleId::Power => power_lines(snapshot, config),
        SystemModuleId::Clock => clock_lines(snapshot),
    };

    let controls = match id {
        SystemModuleId::Keyboard => vec![action_spec(
            id,
            "cycle-layout",
            "Next layout",
            ActionIntent::ToggleKeyboardLayout,
            false,
        )],
        SystemModuleId::Audio => audio_controls(snapshot),
        SystemModuleId::Power => vec![
            action_spec(
                id,
                "profile-prev",
                "Previous profile",
                ActionIntent::CyclePowerProfile {
                    direction: Direction::Previous,
                },
                false,
            ),
            action_spec(
                id,
                "profile-next",
                "Next profile",
                ActionIntent::CyclePowerProfile {
                    direction: Direction::Next,
                },
                false,
            ),
        ],
        SystemModuleId::Clock => clock_controls(snapshot),
        SystemModuleId::Resources | SystemModuleId::Network => Vec::new(),
    };

    SystemPopoverSpec {
        id,
        title: id.title().to_string(),
        lines,
        controls,
        footer: action_spec(
            id,
            "open-luma",
            "Open in Luma",
            ActionIntent::OpenContextQuery {
                query: id.luma_query().to_string(),
            },
            true,
        ),
        error: result.and_then(action_error),
    }
}

fn build_button_spec(
    id: SystemModuleId,
    snapshot: &BarSnapshot,
    config: &AppConfig,
) -> SystemButtonSpec {
    match id {
        SystemModuleId::Keyboard => keyboard_button(snapshot),
        SystemModuleId::Resources => resources_button(snapshot),
        SystemModuleId::Network => network_button(snapshot),
        SystemModuleId::Audio => audio_button(snapshot),
        SystemModuleId::Power => power_button(snapshot, config),
        SystemModuleId::Clock => clock_button(snapshot),
    }
}

fn keyboard_button(snapshot: &BarSnapshot) -> SystemButtonSpec {
    let layout = snapshot
        .system
        .keyboard_layout
        .as_deref()
        .unwrap_or("Unknown layout");
    let mut classes = base_classes(SystemModuleId::Keyboard);
    apply_health(
        &mut classes,
        snapshot.system.source_health.get(&SourceId::Compositor),
    );

    SystemButtonSpec {
        id: SystemModuleId::Keyboard,
        icon_name: "input-keyboard-symbolic".to_string(),
        label: Some(short_layout_label(layout)),
        tooltip: with_health_note(
            format!("Keyboard layout: {layout}"),
            snapshot.system.source_health.get(&SourceId::Compositor),
        ),
        classes,
    }
}

fn resources_button(snapshot: &BarSnapshot) -> SystemButtonSpec {
    let cpu = snapshot.system.resources.cpu_percent;
    let memory = snapshot.system.resources.memory_percent;
    let mut classes = base_classes(SystemModuleId::Resources);
    if let Some(severity) = resource_severity(cpu, memory) {
        classes.push(severity.to_string());
    }
    apply_health(
        &mut classes,
        snapshot.system.source_health.get(&SourceId::Resources),
    );

    let label = match (cpu, memory) {
        (Some(cpu), Some(memory)) => Some(format!("{cpu}/{memory}")),
        (Some(cpu), None) => Some(format!("{cpu}/--")),
        (None, Some(memory)) => Some(format!("--/{memory}")),
        (None, None) => None,
    };

    SystemButtonSpec {
        id: SystemModuleId::Resources,
        icon_name: "utilities-system-monitor-symbolic".to_string(),
        label,
        tooltip: with_health_note(
            format!(
                "CPU {}\nMemory {}",
                percent_or_unavailable(cpu),
                percent_or_unavailable(memory)
            ),
            snapshot.system.source_health.get(&SourceId::Resources),
        ),
        classes,
    }
}

fn network_button(snapshot: &BarSnapshot) -> SystemButtonSpec {
    let health = snapshot.system.source_health.get(&SourceId::Network);
    let mut classes = base_classes(SystemModuleId::Network);
    apply_health(&mut classes, health);

    SystemButtonSpec {
        id: SystemModuleId::Network,
        icon_name: snapshot
            .system
            .network
            .icon_hint
            .clone()
            .unwrap_or_else(|| "network-idle-symbolic".to_string()),
        label: None,
        tooltip: with_health_note(network_tooltip(snapshot), health),
        classes,
    }
}

fn audio_button(snapshot: &BarSnapshot) -> SystemButtonSpec {
    let mut classes = base_classes(SystemModuleId::Audio);
    if !snapshot.system.bluetooth.powered {
        classes.push("inactive".to_string());
    }
    apply_health(
        &mut classes,
        module_health(snapshot, &[SourceId::Audio, SourceId::Bluetooth]),
    );

    SystemButtonSpec {
        id: SystemModuleId::Audio,
        icon_name: audio_icon(&snapshot.system.audio, snapshot.system.bluetooth.powered)
            .to_string(),
        label: None,
        tooltip: with_health_note(
            audio_tooltip(snapshot),
            module_health(snapshot, &[SourceId::Audio, SourceId::Bluetooth]),
        ),
        classes,
    }
}

fn power_button(snapshot: &BarSnapshot, config: &AppConfig) -> SystemButtonSpec {
    let health = snapshot.system.source_health.get(&SourceId::Power);
    let mut classes = base_classes(SystemModuleId::Power);
    if snapshot.system.power.charging {
        classes.push("charging".to_string());
    } else if let Some(severity) = battery_severity_class(snapshot, config) {
        classes.push(severity.to_string());
    }
    apply_health(&mut classes, health);

    let percent = snapshot.system.power.battery_percent;
    SystemButtonSpec {
        id: SystemModuleId::Power,
        icon_name: power_icon(percent, snapshot.system.power.charging).to_string(),
        label: percent.map(|value| format!("{value}%")),
        tooltip: with_health_note(power_tooltip(snapshot), health),
        classes,
    }
}

fn clock_button(snapshot: &BarSnapshot) -> SystemButtonSpec {
    let health = module_health(
        snapshot,
        &[SourceId::Clock, SourceId::Calendar, SourceId::Timers],
    );
    let mut classes = base_classes(SystemModuleId::Clock);
    apply_health(&mut classes, health);

    SystemButtonSpec {
        id: SystemModuleId::Clock,
        icon_name: "preferences-system-time-symbolic".to_string(),
        label: Some(snapshot.system.clock.label.clone()),
        tooltip: with_health_note(format!("Clock: {}", snapshot.system.clock.label), health),
        classes,
    }
}

fn keyboard_lines(snapshot: &BarSnapshot) -> Vec<String> {
    vec![format!(
        "Layout: {}",
        snapshot
            .system
            .keyboard_layout
            .as_deref()
            .unwrap_or("Unknown layout")
    )]
}

fn resource_lines(snapshot: &BarSnapshot) -> Vec<String> {
    vec![
        format!(
            "CPU {}",
            percent_or_unavailable(snapshot.system.resources.cpu_percent)
        ),
        format!(
            "Memory {}",
            percent_or_unavailable(snapshot.system.resources.memory_percent)
        ),
    ]
}

fn network_lines(snapshot: &BarSnapshot) -> Vec<String> {
    let mut lines = vec![network_tooltip(snapshot)];
    if let Some(SourceHealth::Disconnected { message }) =
        snapshot.system.source_health.get(&SourceId::Network)
    {
        lines.push(format!("Dependency: {message}"));
    }
    lines
}

fn audio_lines(snapshot: &BarSnapshot) -> Vec<String> {
    let mut lines = vec![format!(
        "Volume {}",
        percent_or_unavailable(snapshot.system.audio.volume_percent)
    )];

    if snapshot.system.audio.muted {
        lines.push("Muted".to_string());
    }

    if snapshot.system.bluetooth.powered {
        lines.push(
            match snapshot.system.bluetooth.connected_device.as_deref() {
                Some(device) => format!("Bluetooth: {device}"),
                None => "Bluetooth on".to_string(),
            },
        );
    } else {
        lines.push("Bluetooth off".to_string());
    }

    if let Some(media) = snapshot.system.media.as_ref() {
        lines.push(media_summary(media));
    }

    lines
}

fn power_lines(snapshot: &BarSnapshot, _config: &AppConfig) -> Vec<String> {
    vec![
        match snapshot.system.power.battery_percent {
            Some(percent) if snapshot.system.power.charging => {
                format!("Battery {percent}% (charging)")
            }
            Some(percent) => format!("Battery {percent}%"),
            None => "Battery unavailable".to_string(),
        },
        format!(
            "Profile: {}",
            power_profile_label(snapshot.system.power.profile.clone())
        ),
    ]
}

fn clock_lines(snapshot: &BarSnapshot) -> Vec<String> {
    let mut lines = vec![format!("Time: {}", snapshot.system.clock.label)];

    if let Some(calendar) = snapshot.system.calendar.as_ref() {
        lines.push(format!("Next: {}", calendar.title));
    }

    if let Some(timer) = snapshot.system.timers.first() {
        lines.push(timer_line(timer));
    }

    lines
}

fn audio_controls(snapshot: &BarSnapshot) -> Vec<SystemActionSpec> {
    if snapshot.system.media.is_none() {
        return Vec::new();
    }

    vec![
        action_spec(
            SystemModuleId::Audio,
            "previous",
            "Previous",
            ActionIntent::ControlMedia(MediaControlAction::Previous),
            false,
        ),
        action_spec(
            SystemModuleId::Audio,
            "play-pause",
            "Play/Pause",
            ActionIntent::ControlMedia(MediaControlAction::PlayPause),
            false,
        ),
        action_spec(
            SystemModuleId::Audio,
            "next",
            "Next",
            ActionIntent::ControlMedia(MediaControlAction::Next),
            false,
        ),
    ]
}

fn clock_controls(snapshot: &BarSnapshot) -> Vec<SystemActionSpec> {
    let mut controls = vec![action_spec(
        SystemModuleId::Clock,
        "start-5m",
        "Start 5m timer",
        ActionIntent::StartTimer {
            label: "Quick timer".to_string(),
            duration_seconds: 5 * 60,
        },
        false,
    )];

    if let Some(timer) = snapshot.system.timers.first() {
        if timer.completed {
            controls.push(action_spec(
                SystemModuleId::Clock,
                "cancel-timer",
                "Clear timer",
                ActionIntent::CancelTimer {
                    id: timer.id.clone(),
                },
                false,
            ));
        } else if timer.target_epoch.is_some() {
            controls.push(action_spec(
                SystemModuleId::Clock,
                "pause-timer",
                "Pause timer",
                ActionIntent::PauseTimer {
                    id: timer.id.clone(),
                },
                false,
            ));
        } else {
            controls.push(action_spec(
                SystemModuleId::Clock,
                "resume-timer",
                "Resume timer",
                ActionIntent::ResumeTimer {
                    id: timer.id.clone(),
                },
                false,
            ));
        }
    }

    controls
}

fn action_spec(
    module_id: SystemModuleId,
    suffix: &str,
    label: &str,
    intent: ActionIntent,
    closes_popover: bool,
) -> SystemActionSpec {
    SystemActionSpec {
        label: label.to_string(),
        origin: format!("system-popover:{}:{suffix}", module_id.as_str()),
        intent,
        closes_popover,
    }
}

fn action_error(result: &ActionResult) -> Option<String> {
    match result {
        ActionResult::Failed { detail, .. } => Some(detail.clone()),
        ActionResult::Completed => None,
    }
}

fn short_layout_label(layout: &str) -> String {
    let lower = layout.to_ascii_lowercase();
    if lower.contains("dvorak") {
        "DV".to_string()
    } else if lower.contains("us") {
        "US".to_string()
    } else {
        layout
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .find(|segment| !segment.is_empty())
            .map(|segment| {
                segment
                    .chars()
                    .take(2)
                    .collect::<String>()
                    .to_ascii_uppercase()
            })
            .filter(|segment| !segment.is_empty())
            .unwrap_or_else(|| "KB".to_string())
    }
}

fn resource_severity(cpu: Option<u8>, memory: Option<u8>) -> Option<&'static str> {
    let pressure = cpu.into_iter().chain(memory).max()?;
    if pressure >= 90 {
        Some("critical")
    } else if pressure >= 75 {
        Some("warning")
    } else {
        None
    }
}

fn audio_icon(audio: &AudioState, bluetooth_powered: bool) -> &'static str {
    if !bluetooth_powered {
        "bluetooth-disabled-symbolic"
    } else if audio.muted {
        "audio-volume-muted-symbolic"
    } else {
        match audio.volume_percent.unwrap_or_default() {
            0 => "audio-volume-muted-symbolic",
            1..=33 => "audio-volume-low-symbolic",
            34..=66 => "audio-volume-medium-symbolic",
            _ => "audio-volume-high-symbolic",
        }
    }
}

fn power_icon(percent: Option<u8>, charging: bool) -> &'static str {
    match (percent.unwrap_or(100), charging) {
        (_, true) => "battery-level-100-charging-symbolic",
        (0..=10, false) => "battery-level-10-symbolic",
        (11..=30, false) => "battery-level-30-symbolic",
        (31..=60, false) => "battery-level-50-symbolic",
        (61..=80, false) => "battery-level-80-symbolic",
        _ => "battery-level-100-symbolic",
    }
}

fn network_tooltip(snapshot: &BarSnapshot) -> String {
    match snapshot.system.network.connectivity {
        ConnectivityState::Connected => snapshot
            .system
            .network
            .label
            .as_ref()
            .map(|label| format!("Network: {label}"))
            .unwrap_or_else(|| "Network connected".to_string()),
        ConnectivityState::Connecting => "Network connecting".to_string(),
        ConnectivityState::Disconnected => "Network disconnected".to_string(),
        ConnectivityState::Unknown => "Network status unavailable".to_string(),
    }
}

fn audio_tooltip(snapshot: &BarSnapshot) -> String {
    let mut parts = vec![format!(
        "Volume {}",
        percent_or_unavailable(snapshot.system.audio.volume_percent)
    )];

    if snapshot.system.audio.muted {
        parts.push("Muted".to_string());
    }

    if snapshot.system.bluetooth.powered {
        parts.push(
            snapshot
                .system
                .bluetooth
                .connected_device
                .as_ref()
                .map(|device| format!("Bluetooth: {device}"))
                .unwrap_or_else(|| "Bluetooth on".to_string()),
        );
    } else {
        parts.push("Bluetooth off".to_string());
    }

    if let Some(media) = snapshot.system.media.as_ref() {
        parts.push(media_summary(media));
    }

    parts.join("\n")
}

fn power_tooltip(snapshot: &BarSnapshot) -> String {
    let battery = match snapshot.system.power.battery_percent {
        Some(percent) if snapshot.system.power.charging => format!("Battery {percent}% (charging)"),
        Some(percent) => format!("Battery {percent}%"),
        None => "Battery unavailable".to_string(),
    };
    format!(
        "{battery}\nProfile: {}",
        power_profile_label(snapshot.system.power.profile.clone())
    )
}

fn timer_line(timer: &TimerState) -> String {
    if timer.completed {
        format!("Timer {} completed", timer.label)
    } else {
        format!(
            "Timer {} {} remaining",
            timer.label,
            format_duration(timer.remaining_seconds)
        )
    }
}

fn media_summary(media: &crate::MediaState) -> String {
    let status = match media.status {
        PlaybackStatus::Playing => "Playing",
        PlaybackStatus::Paused => "Paused",
        PlaybackStatus::Stopped => "Stopped",
    };
    match (&media.title, &media.artist) {
        (Some(title), Some(artist)) => format!("{status}: {title} - {artist}"),
        (Some(title), None) => format!("{status}: {title}"),
        (None, Some(artist)) => format!("{status}: {artist}"),
        (None, None) => format!("{status}: {}", media.player),
    }
}

fn power_profile_label(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "Performance",
        PowerProfile::Balanced => "Balanced",
        PowerProfile::PowerSaver => "Power Saver",
    }
}

fn battery_severity_class(snapshot: &BarSnapshot, config: &AppConfig) -> Option<&'static str> {
    let percent = snapshot.system.power.battery_percent?;
    if percent <= config.thresholds.battery_critical_percent {
        Some("critical")
    } else if percent <= config.thresholds.battery_low_percent {
        Some("warning")
    } else {
        None
    }
}

fn module_health<'a>(snapshot: &'a BarSnapshot, ids: &[SourceId]) -> Option<&'a SourceHealth> {
    ids.iter()
        .filter_map(|id| snapshot.system.source_health.get(id))
        .find(|health| matches!(health, SourceHealth::Disconnected { .. }))
        .or_else(|| {
            ids.iter()
                .filter_map(|id| snapshot.system.source_health.get(id))
                .find(|health| matches!(health, SourceHealth::Stale { .. }))
        })
}

fn apply_health(classes: &mut Vec<String>, health: Option<&SourceHealth>) {
    match health {
        Some(SourceHealth::Stale { .. }) => classes.push("stale".to_string()),
        Some(SourceHealth::Disconnected { .. }) => classes.push("disconnected".to_string()),
        Some(SourceHealth::Healthy) | None => {}
    }
}

fn with_health_note(base: String, health: Option<&SourceHealth>) -> String {
    match health {
        Some(SourceHealth::Stale { .. }) => format!("{base}\nStale"),
        Some(SourceHealth::Disconnected { message }) => format!("{base}\nDependency: {message}"),
        Some(SourceHealth::Healthy) | None => base,
    }
}

fn base_classes(id: SystemModuleId) -> Vec<String> {
    vec!["system-module".to_string(), id.as_str().to_string()]
}

fn percent_or_unavailable(value: Option<u8>) -> String {
    value
        .map(|percent| format!("{percent}%"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        ActionResult, AppConfig, AudioState, BarSnapshot, BluetoothState, ClockState,
        ConnectivityState, MediaState, NetworkState, PlaybackStatus, PowerProfile, PowerState,
        ResourceState, SourceHealth, SourceId,
    };

    use super::{SystemModuleId, build_popover_spec, build_system_cluster};

    #[test]
    fn keyboard_button_compacts_us_and_dvorak_layout_labels() {
        let mut snapshot = snapshot();
        snapshot.system.keyboard_layout = Some("English (US)".to_string());

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let keyboard = cluster.module(SystemModuleId::Keyboard);
        assert_eq!(keyboard.label.as_deref(), Some("US"));
        assert!(keyboard.tooltip.contains("English (US)"));

        snapshot.system.keyboard_layout = Some("English (Dvorak)".to_string());
        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let keyboard = cluster.module(SystemModuleId::Keyboard);
        assert_eq!(keyboard.label.as_deref(), Some("DV"));
        assert!(keyboard.tooltip.contains("English (Dvorak)"));
    }

    #[test]
    fn resources_button_marks_cpu_and_memory_pressure() {
        let mut snapshot = snapshot();
        snapshot.system.resources = ResourceState {
            cpu_percent: Some(91),
            memory_percent: Some(76),
        };

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let resources = cluster.module(SystemModuleId::Resources);

        assert_eq!(resources.label.as_deref(), Some("91/76"));
        assert!(resources.classes.iter().any(|class| class == "critical"));
        assert!(resources.tooltip.contains("CPU 91%"));
        assert!(resources.tooltip.contains("Memory 76%"));
    }

    #[test]
    fn network_button_distinguishes_connected_and_disconnected_states() {
        let mut snapshot = snapshot();
        snapshot.system.network = NetworkState {
            connectivity: ConnectivityState::Connected,
            icon_hint: Some("network-wireless-signal-good-symbolic".to_string()),
            label: Some("Office Wi-Fi".to_string()),
            wifi_enabled: Some(true),
        };

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let network = cluster.module(SystemModuleId::Network);
        assert!(network.tooltip.contains("Office Wi-Fi"));
        assert!(!network.classes.iter().any(|class| class == "disconnected"));

        snapshot.system.source_health.insert(
            SourceId::Network,
            SourceHealth::Disconnected {
                message: "NetworkManager unavailable".to_string(),
            },
        );
        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let network = cluster.module(SystemModuleId::Network);
        assert!(network.classes.iter().any(|class| class == "disconnected"));
        assert!(network.tooltip.contains("NetworkManager unavailable"));
    }

    #[test]
    fn bluetooth_button_shows_powered_off_state_without_source_failure() {
        let mut snapshot = snapshot();
        snapshot.system.bluetooth = BluetoothState {
            powered: false,
            connected_device: None,
            audio_device: None,
        };
        snapshot.system.audio = AudioState {
            volume_percent: Some(42),
            muted: false,
        };

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let audio = cluster.module(SystemModuleId::Audio);

        assert!(audio.classes.iter().any(|class| class == "inactive"));
        assert!(!audio.classes.iter().any(|class| class == "disconnected"));
        assert!(audio.tooltip.contains("Bluetooth off"));
        assert!(audio.tooltip.contains("42%"));
    }

    #[test]
    fn power_button_marks_charging_low_and_critical_battery_states() {
        let mut snapshot = snapshot();
        snapshot.system.power = PowerState {
            battery_percent: Some(88),
            charging: true,
            profile: PowerProfile::Balanced,
            changed_at: 0,
        };

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let power = cluster.module(SystemModuleId::Power);
        assert_eq!(power.label.as_deref(), Some("88%"));
        assert!(power.classes.iter().any(|class| class == "charging"));

        snapshot.system.power.battery_percent = Some(15);
        snapshot.system.power.charging = false;
        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let power = cluster.module(SystemModuleId::Power);
        assert!(power.classes.iter().any(|class| class == "warning"));

        snapshot.system.power.battery_percent = Some(7);
        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let power = cluster.module(SystemModuleId::Power);
        assert!(power.classes.iter().any(|class| class == "critical"));
    }

    #[test]
    fn stale_health_adds_stale_class_to_the_affected_module() {
        let mut snapshot = snapshot();
        snapshot.system.source_health.insert(
            SourceId::Resources,
            SourceHealth::Stale { since_epoch: 123 },
        );

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let resources = cluster.module(SystemModuleId::Resources);

        assert!(resources.classes.iter().any(|class| class == "stale"));
        assert!(resources.tooltip.contains("Stale"));
    }

    #[test]
    fn audio_popover_surfaces_media_and_last_action_failure() {
        let mut snapshot = snapshot();
        snapshot.system.media = Some(MediaState {
            player: "spotify".to_string(),
            status: PlaybackStatus::Playing,
            title: Some("Says".to_string()),
            artist: Some("Nils Frahm".to_string()),
            changed_at: 0,
        });

        let popover = build_popover_spec(
            SystemModuleId::Audio,
            &snapshot,
            &AppConfig::default(),
            Some(&ActionResult::Failed {
                summary: "Action failed".to_string(),
                detail: "playerctl missing".to_string(),
            }),
        );

        assert!(popover.lines.iter().any(|line| line.contains("Nils Frahm")));
        assert_eq!(popover.error.as_deref(), Some("playerctl missing"));
    }

    fn snapshot() -> BarSnapshot {
        BarSnapshot {
            outputs: BTreeMap::new(),
            focused_output: None,
            system: crate::SystemState {
                clock: ClockState {
                    epoch_seconds: 1_800_000_000,
                    label: "12:00".to_string(),
                },
                network: NetworkState {
                    connectivity: ConnectivityState::Disconnected,
                    icon_hint: Some("network-offline-symbolic".to_string()),
                    label: None,
                    wifi_enabled: Some(false),
                },
                ..crate::SystemState::default()
            },
            ..BarSnapshot::default()
        }
    }
}
