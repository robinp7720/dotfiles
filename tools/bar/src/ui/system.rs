use crate::{
    AppConfig, AudioState, BarSnapshot, ConnectivityState, PlaybackStatus, PowerProfile,
    SourceHealth, SourceId,
};

use super::control_center::{ControlCenterSpec, build_control_center_spec};

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
pub struct SystemModuleSpec {
    pub button: SystemButtonSpec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemCluster {
    modules: Vec<SystemModuleSpec>,
    control_center: ControlCenterSpec,
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

    pub fn control_center(&self) -> &ControlCenterSpec {
        &self.control_center
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
        })
        .collect();

    SystemCluster {
        modules,
        control_center: build_control_center_spec(snapshot),
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
    if snapshot.system.bluetooth.available && !snapshot.system.bluetooth.powered {
        classes.push("inactive".to_string());
    }
    let health = if snapshot.system.bluetooth.available {
        module_health(snapshot, &[SourceId::Audio, SourceId::Bluetooth])
    } else {
        snapshot.system.source_health.get(&SourceId::Audio)
    };
    apply_health(&mut classes, health);

    SystemButtonSpec {
        id: SystemModuleId::Audio,
        icon_name: audio_icon(&snapshot.system.audio, snapshot.system.bluetooth.powered)
            .to_string(),
        label: None,
        tooltip: with_health_note(audio_tooltip(snapshot), health),
        classes,
    }
}

fn power_button(snapshot: &BarSnapshot, config: &AppConfig) -> SystemButtonSpec {
    let health = snapshot.system.source_health.get(&SourceId::Power);
    let mut classes = base_classes(SystemModuleId::Power);
    if snapshot.system.power.battery_present {
        if snapshot.system.power.charging {
            classes.push("charging".to_string());
        } else if let Some(severity) = battery_severity_class(snapshot, config) {
            classes.push(severity.to_string());
        }
    }
    apply_health(&mut classes, health);

    let percent = snapshot.system.power.battery_percent;
    let (icon_name, label) = if snapshot.system.power.battery_present {
        (
            power_icon(percent, snapshot.system.power.charging),
            percent.map(|value| format!("{value}%")),
        )
    } else {
        (power_profile_icon(&snapshot.system.power.profile), None)
    };
    SystemButtonSpec {
        id: SystemModuleId::Power,
        icon_name: icon_name.to_string(),
        label,
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
    } else if snapshot.system.bluetooth.available {
        parts.push("Bluetooth off".to_string());
    }

    if let Some(media) = snapshot.system.media.as_ref() {
        parts.push(media_summary(media));
    }

    parts.join("\n")
}

fn power_tooltip(snapshot: &BarSnapshot) -> String {
    if !snapshot.system.power.battery_present {
        return format!(
            "Power profile: {}",
            power_profile_label(snapshot.system.power.profile.clone())
        );
    }
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

fn power_profile_icon(profile: &PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "power-profile-performance-symbolic",
        PowerProfile::Balanced => "power-profile-balanced-symbolic",
        PowerProfile::PowerSaver => "power-profile-power-saver-symbolic",
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AppConfig, AudioState, BarSnapshot, BluetoothState, ClockState, ConnectivityState,
        NetworkState, PowerProfile, PowerState, ResourceState, SourceHealth, SourceId,
    };

    use super::{SystemModuleId, build_system_cluster};

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
            wifi_available: true,
            ethernet_available: true,
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
            available: true,
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
    fn audio_button_ignores_bluetooth_when_no_adapter_exists() {
        let mut snapshot = snapshot();
        snapshot.system.audio = AudioState {
            volume_percent: Some(42),
            muted: false,
        };

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let audio = cluster.module(SystemModuleId::Audio);

        assert!(!audio.classes.iter().any(|class| class == "inactive"));
        assert!(!audio.tooltip.contains("Bluetooth"));
    }

    #[test]
    fn power_button_marks_charging_low_and_critical_battery_states() {
        let mut snapshot = snapshot();
        snapshot.system.power = PowerState {
            battery_present: true,
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
    fn desktop_power_button_uses_profile_instead_of_fake_battery() {
        let mut snapshot = snapshot();
        snapshot.system.power.profile = PowerProfile::Performance;

        let cluster = build_system_cluster(&snapshot, &AppConfig::default());
        let power = cluster.module(SystemModuleId::Power);

        assert_eq!(power.icon_name, "power-profile-performance-symbolic");
        assert_eq!(power.label, None);
        assert_eq!(power.tooltip, "Power profile: Performance");
        assert!(!power.classes.iter().any(|class| class == "charging"));
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
                    wifi_available: true,
                    ethernet_available: true,
                    wifi_enabled: Some(false),
                },
                ..crate::SystemState::default()
            },
            ..BarSnapshot::default()
        }
    }
}
