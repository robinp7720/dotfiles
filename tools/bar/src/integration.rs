use anyhow::{Result, bail};

use crate::{
    ActionIntent, BarSnapshot, ContextAction, ContextActionSpec, ContextHealth, ContextSnapshot,
    DesktopContext, MediaControlAction, PlaybackStatus, PowerProfile, SourceHealth, SourceId,
};

pub fn context_snapshots(
    snapshot: &BarSnapshot,
    requested: Option<DesktopContext>,
) -> Vec<ContextSnapshot> {
    requested
        .map(|context| vec![context_snapshot(snapshot, context)])
        .unwrap_or_else(|| {
            DesktopContext::ALL
                .into_iter()
                .map(|context| context_snapshot(snapshot, context))
                .collect()
        })
}

pub fn context_snapshot(snapshot: &BarSnapshot, context: DesktopContext) -> ContextSnapshot {
    match context {
        DesktopContext::Overview => overview_snapshot(snapshot),
        DesktopContext::Keyboard => keyboard_snapshot(snapshot),
        DesktopContext::Resources => resources_snapshot(snapshot),
        DesktopContext::Network => network_snapshot(snapshot),
        DesktopContext::Bluetooth => bluetooth_snapshot(snapshot),
        DesktopContext::Audio => audio_snapshot(snapshot),
        DesktopContext::Power => power_snapshot(snapshot),
        DesktopContext::Clock => clock_snapshot(snapshot),
    }
}

pub fn intent_for_context_action(
    snapshot: &BarSnapshot,
    action: ContextAction,
) -> Result<ActionIntent> {
    let system = &snapshot.system;
    Ok(match action {
        ContextAction::SelectKeyboardLayout { index } => {
            if !system
                .keyboard_layout
                .layouts
                .iter()
                .any(|layout| layout.index == index)
            {
                bail!("keyboard layout is no longer available");
            }
            ActionIntent::SelectKeyboardLayout { index }
        }
        ContextAction::SetWifiEnabled { enabled } => {
            if !system.network.wifi_available {
                bail!("Wi-Fi is not available on this system");
            }
            ActionIntent::SetWifiEnabled { enabled }
        }
        ContextAction::SetBluetoothPowered { powered } => {
            if !system.bluetooth.available {
                bail!("Bluetooth is not available on this system");
            }
            ActionIntent::SetBluetoothPowered { powered }
        }
        ContextAction::ConnectBluetoothDevice { address } => {
            if !system
                .bluetooth
                .devices
                .iter()
                .any(|device| device.address == address && device.paired && !device.connected)
            {
                bail!("Bluetooth device is no longer available to connect");
            }
            ActionIntent::ConnectBluetoothDevice { address }
        }
        ContextAction::DisconnectBluetoothDevice { address } => {
            if !system
                .bluetooth
                .devices
                .iter()
                .any(|device| device.address == address && device.connected)
            {
                bail!("Bluetooth device is no longer connected");
            }
            ActionIntent::DisconnectBluetoothDevice { address }
        }
        ContextAction::SetVolumePercent { percent } => ActionIntent::SetVolumePercent {
            percent: percent.min(100),
        },
        ContextAction::ToggleMute => ActionIntent::ToggleMute,
        ContextAction::SetAudioOutput { sink_name } => {
            if !system
                .audio
                .outputs
                .iter()
                .any(|output| output.name == sink_name)
            {
                bail!("audio output is no longer available");
            }
            ActionIntent::SetAudioOutput { sink_name }
        }
        ContextAction::ControlMedia { player, action } => {
            if system.media.as_ref().map(|media| media.player.as_str()) != Some(player.as_str()) {
                bail!("media player is no longer active");
            }
            ActionIntent::ControlMedia { player, action }
        }
        ContextAction::SetBrightnessPercent { device, percent } => {
            if system.brightness.device.as_deref() != Some(device.as_str()) {
                bail!("brightness device is no longer available");
            }
            ActionIntent::SetBrightnessPercent {
                device,
                percent: percent.min(100),
            }
        }
        ContextAction::SetPowerProfile { profile } => ActionIntent::SetPowerProfile { profile },
        ContextAction::PauseTimer { id } => {
            if !system
                .timers
                .iter()
                .any(|timer| timer.id == id && !timer.completed && timer.target_epoch.is_some())
            {
                bail!("timer is no longer running");
            }
            ActionIntent::PauseTimer { id }
        }
        ContextAction::ResumeTimer { id } => {
            if !system
                .timers
                .iter()
                .any(|timer| timer.id == id && !timer.completed && timer.target_epoch.is_none())
            {
                bail!("timer is no longer paused");
            }
            ActionIntent::ResumeTimer { id }
        }
        ContextAction::CancelTimer { id } => {
            if !system.timers.iter().any(|timer| timer.id == id) {
                bail!("timer no longer exists");
            }
            ActionIntent::CancelTimer { id }
        }
    })
}

fn overview_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let system = &snapshot.system;
    let network = system.network.label.as_deref().unwrap_or("Offline");
    let volume = system
        .audio
        .volume_percent
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "Audio unavailable".to_string());
    let summary = format!("{network} · {volume}");
    let detail = system
        .media
        .as_ref()
        .and_then(|media| media.title.as_deref())
        .map(|title| format!("Now playing: {title}"))
        .unwrap_or_else(|| "Live desktop controls".to_string());

    let mut actions = Vec::new();
    actions.extend(audio_snapshot(snapshot).actions.into_iter().take(2));
    actions.extend(network_snapshot(snapshot).actions.into_iter().take(1));
    actions.extend(bluetooth_snapshot(snapshot).actions.into_iter().take(1));
    actions.extend(power_snapshot(snapshot).actions.into_iter().take(1));

    make_snapshot(
        DesktopContext::Overview,
        "Quick Settings",
        "preferences-system-symbolic",
        summary,
        detail,
        ContextHealth::Healthy,
        actions,
    )
}

fn keyboard_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.keyboard_layout;
    let summary = state
        .current_name
        .clone()
        .unwrap_or_else(|| "Layout unavailable".to_string());
    let actions = state
        .layouts
        .iter()
        .filter(|layout| Some(layout.index) != state.current_index)
        .map(|layout| {
            action(
                format!("Use {}", layout.name),
                format_layout(layout.layout.as_deref(), layout.variant.as_deref()),
                "input-keyboard-symbolic",
                None,
                ContextAction::SelectKeyboardLayout {
                    index: layout.index,
                },
            )
        })
        .collect();
    make_snapshot(
        DesktopContext::Keyboard,
        "Keyboard",
        "input-keyboard-symbolic",
        summary,
        "Active layout and variant",
        source_health(snapshot, SourceId::Compositor),
        actions,
    )
}

fn resources_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.resources;
    let cpu = state
        .cpu_percent
        .map_or_else(|| "—".to_string(), |v| format!("{v}%"));
    let memory = state
        .memory_percent
        .map_or_else(|| "—".to_string(), |v| format!("{v}%"));
    make_snapshot(
        DesktopContext::Resources,
        "System Resources",
        "utilities-system-monitor-symbolic",
        format!("CPU {cpu} · Memory {memory}"),
        "Current system load",
        source_health(snapshot, SourceId::Resources),
        Vec::new(),
    )
}

fn network_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.network;
    let mut actions = Vec::new();
    if state.wifi_available
        && let Some(enabled) = state.wifi_enabled
    {
        actions.push(action(
            if enabled {
                "Turn off Wi-Fi"
            } else {
                "Turn on Wi-Fi"
            },
            state
                .label
                .clone()
                .unwrap_or_else(|| "Wireless network".to_string()),
            if enabled {
                "network-wireless-offline-symbolic"
            } else {
                "network-wireless-symbolic"
            },
            None,
            ContextAction::SetWifiEnabled { enabled: !enabled },
        ));
    }
    let detail = state
        .interface
        .as_deref()
        .map(|interface| format!("Interface {interface}"))
        .unwrap_or_else(|| "Connectivity and traffic".to_string());
    make_snapshot(
        DesktopContext::Network,
        "Network",
        state
            .icon_hint
            .as_deref()
            .unwrap_or("network-offline-symbolic"),
        state
            .label
            .clone()
            .unwrap_or_else(|| "Disconnected".to_string()),
        detail,
        source_health(snapshot, SourceId::Network),
        actions,
    )
}

fn bluetooth_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.bluetooth;
    let mut actions = Vec::new();
    if state.available {
        actions.push(action(
            if state.powered {
                "Turn off Bluetooth"
            } else {
                "Turn on Bluetooth"
            },
            if state.powered {
                "Bluetooth is on"
            } else {
                "Bluetooth is off"
            },
            "bluetooth-active-symbolic",
            None,
            ContextAction::SetBluetoothPowered {
                powered: !state.powered,
            },
        ));
    }
    if state.powered {
        for device in state.devices.iter().filter(|device| device.paired) {
            let (title, action_value) = if device.connected {
                (
                    format!("Disconnect {}", device.name),
                    ContextAction::DisconnectBluetoothDevice {
                        address: device.address.clone(),
                    },
                )
            } else {
                (
                    format!("Connect {}", device.name),
                    ContextAction::ConnectBluetoothDevice {
                        address: device.address.clone(),
                    },
                )
            };
            actions.push(action(
                title,
                if device.connected {
                    "Connected"
                } else {
                    "Saved device"
                },
                &device.icon_name,
                device.battery_percent.map(|battery| format!("{battery}%")),
                action_value,
            ));
        }
    }
    let connected = state
        .devices
        .iter()
        .filter(|device| device.connected)
        .count();
    make_snapshot(
        DesktopContext::Bluetooth,
        "Bluetooth",
        "bluetooth-active-symbolic",
        if !state.available {
            "Unavailable".to_string()
        } else if !state.powered {
            "Off".to_string()
        } else if connected == 0 {
            "On · No devices connected".to_string()
        } else {
            format!(
                "{connected} device{} connected",
                if connected == 1 { "" } else { "s" }
            )
        },
        "Saved devices and controller power",
        source_health(snapshot, SourceId::Bluetooth),
        actions,
    )
}

fn audio_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.audio;
    let mut actions = Vec::new();
    if let Some(volume) = state.volume_percent {
        actions.push(action(
            if state.muted {
                "Unmute audio"
            } else {
                "Mute audio"
            },
            format!("Current volume {volume}%"),
            if state.muted {
                "audio-volume-muted-symbolic"
            } else {
                "audio-volume-high-symbolic"
            },
            None,
            ContextAction::ToggleMute,
        ));
        for (title, percent, icon) in [
            (
                "Lower volume",
                volume.saturating_sub(5),
                "audio-volume-low-symbolic",
            ),
            (
                "Raise volume",
                volume.saturating_add(5).min(100),
                "audio-volume-high-symbolic",
            ),
        ] {
            actions.push(action(
                title,
                format!("Set volume to {percent}%"),
                icon,
                Some(format!("{percent}%")),
                ContextAction::SetVolumePercent { percent },
            ));
        }
    }
    for output in &state.outputs {
        if !output.is_default {
            actions.push(action(
                format!(
                    "Use {}",
                    output.alias.as_deref().unwrap_or(&output.description)
                ),
                output
                    .port_description
                    .clone()
                    .unwrap_or_else(|| output.description.clone()),
                "audio-speakers-symbolic",
                None,
                ContextAction::SetAudioOutput {
                    sink_name: output.name.clone(),
                },
            ));
        }
    }
    if let Some(media) = &snapshot.system.media {
        for (title, media_action, icon) in [
            (
                if media.status == PlaybackStatus::Playing {
                    "Pause media"
                } else {
                    "Play media"
                },
                MediaControlAction::PlayPause,
                if media.status == PlaybackStatus::Playing {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                },
            ),
            (
                "Next track",
                MediaControlAction::Next,
                "media-skip-forward-symbolic",
            ),
            (
                "Previous track",
                MediaControlAction::Previous,
                "media-skip-backward-symbolic",
            ),
        ] {
            actions.push(action(
                title,
                media.title.clone().unwrap_or_else(|| media.player.clone()),
                icon,
                None,
                ContextAction::ControlMedia {
                    player: media.player.clone(),
                    action: media_action,
                },
            ));
        }
    }
    let default_output = state.outputs.iter().find(|output| output.is_default);
    make_snapshot(
        DesktopContext::Audio,
        "Audio",
        if state.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        },
        state.volume_percent.map_or_else(
            || "Unavailable".to_string(),
            |volume| {
                if state.muted {
                    format!("Muted · {volume}%")
                } else {
                    format!("Volume {volume}%")
                }
            },
        ),
        default_output
            .map(|output| {
                output
                    .alias
                    .clone()
                    .unwrap_or_else(|| output.description.clone())
            })
            .unwrap_or_else(|| "Default audio output".to_string()),
        source_health(snapshot, SourceId::Audio),
        actions,
    )
}

fn power_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let state = &snapshot.system.power;
    let actions = [
        PowerProfile::Performance,
        PowerProfile::Balanced,
        PowerProfile::PowerSaver,
    ]
    .into_iter()
    .filter(|profile| profile != &state.profile)
    .map(|profile| {
        action(
            format!("Use {}", power_profile_label(&profile)),
            "Change the active power profile",
            power_profile_icon(&profile),
            None,
            ContextAction::SetPowerProfile { profile },
        )
    })
    .collect();
    let detail = if state.battery_present {
        state.battery_percent.map_or_else(
            || "Battery present".to_string(),
            |battery| {
                format!(
                    "Battery {battery}%{}",
                    if state.charging { " · Charging" } else { "" }
                )
            },
        )
    } else {
        "Desktop power profile".to_string()
    };
    make_snapshot(
        DesktopContext::Power,
        "Power",
        power_profile_icon(&state.profile),
        power_profile_label(&state.profile),
        detail,
        source_health(snapshot, SourceId::Power),
        actions,
    )
}

fn clock_snapshot(snapshot: &BarSnapshot) -> ContextSnapshot {
    let system = &snapshot.system;
    let mut actions = Vec::new();
    for timer in &system.timers {
        if !timer.completed {
            let (title, timer_action) = if timer.target_epoch.is_some() {
                (
                    format!("Pause {}", timer.label),
                    ContextAction::PauseTimer {
                        id: timer.id.clone(),
                    },
                )
            } else {
                (
                    format!("Resume {}", timer.label),
                    ContextAction::ResumeTimer {
                        id: timer.id.clone(),
                    },
                )
            };
            actions.push(action(
                title,
                format_duration(timer.remaining_seconds),
                "alarm-symbolic",
                Some(format_duration(timer.remaining_seconds)),
                timer_action,
            ));
            actions.push(action(
                format!("Cancel {}", timer.label),
                "Remove this timer",
                "edit-delete-symbolic",
                None,
                ContextAction::CancelTimer {
                    id: timer.id.clone(),
                },
            ));
        }
    }
    let detail = system.calendar.as_ref().map_or_else(
        || "Calendar and focus timers".to_string(),
        |event| format!("Next: {}", event.title),
    );
    make_snapshot(
        DesktopContext::Clock,
        "Time & Focus",
        "alarm-symbolic",
        system.clock.label.clone(),
        detail,
        source_health(snapshot, SourceId::Clock),
        actions,
    )
}

fn make_snapshot(
    context: DesktopContext,
    title: impl Into<String>,
    icon_name: impl Into<String>,
    summary: impl Into<String>,
    detail: impl Into<String>,
    health: ContextHealth,
    actions: Vec<ContextActionSpec>,
) -> ContextSnapshot {
    ContextSnapshot {
        context,
        title: title.into(),
        icon_name: icon_name.into(),
        summary: summary.into(),
        detail: detail.into(),
        health,
        actions,
    }
}

fn action(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    icon_name: impl Into<String>,
    accessory: Option<String>,
    action: ContextAction,
) -> ContextActionSpec {
    ContextActionSpec {
        title: title.into(),
        subtitle: subtitle.into(),
        icon_name: icon_name.into(),
        accessory,
        action,
    }
}

fn source_health(snapshot: &BarSnapshot, source: SourceId) -> ContextHealth {
    match snapshot.system.source_health.get(&source) {
        Some(SourceHealth::Disconnected { .. }) => ContextHealth::Unavailable,
        Some(SourceHealth::Stale { .. }) => ContextHealth::Degraded,
        Some(SourceHealth::Healthy) | None => ContextHealth::Healthy,
    }
}

fn format_layout(layout: Option<&str>, variant: Option<&str>) -> String {
    match (layout, variant.filter(|value| !value.is_empty())) {
        (Some(layout), Some(variant)) => format!("{layout} · {variant}"),
        (Some(layout), None) => layout.to_string(),
        (None, Some(variant)) => variant.to_string(),
        (None, None) => "Keyboard layout".to_string(),
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

fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes > 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{context_snapshot, intent_for_context_action};
    use crate::{
        ActionIntent, AudioOutputState, ContextAction, DesktopContext, KeyboardLayoutOption,
        PowerProfile,
    };

    #[test]
    fn audio_context_uses_live_output_and_volume_state() {
        let mut snapshot = crate::BarSnapshot::default();
        snapshot.system.audio.volume_percent = Some(42);
        snapshot.system.audio.outputs.push(AudioOutputState {
            name: "sink.headphones".to_string(),
            description: "Headphones".to_string(),
            is_default: true,
            ..Default::default()
        });

        let context = context_snapshot(&snapshot, DesktopContext::Audio);
        assert_eq!(context.summary, "Volume 42%");
        assert_eq!(context.detail, "Headphones");
        assert!(
            context
                .actions
                .iter()
                .any(|action| action.title == "Mute audio")
        );
    }

    #[test]
    fn stale_context_targets_are_rejected() {
        let snapshot = crate::BarSnapshot::default();
        let result = intent_for_context_action(
            &snapshot,
            ContextAction::SetAudioOutput {
                sink_name: "gone".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn valid_layout_and_profile_actions_map_to_typed_intents() {
        let mut snapshot = crate::BarSnapshot::default();
        snapshot
            .system
            .keyboard_layout
            .layouts
            .push(KeyboardLayoutOption {
                index: 2,
                name: "English (intl.)".to_string(),
                layout: Some("us".to_string()),
                variant: Some("intl".to_string()),
            });
        assert_eq!(
            intent_for_context_action(&snapshot, ContextAction::SelectKeyboardLayout { index: 2 },)
                .unwrap(),
            ActionIntent::SelectKeyboardLayout { index: 2 }
        );
        assert_eq!(
            intent_for_context_action(
                &snapshot,
                ContextAction::SetPowerProfile {
                    profile: PowerProfile::PowerSaver
                },
            )
            .unwrap(),
            ActionIntent::SetPowerProfile {
                profile: PowerProfile::PowerSaver
            }
        );
    }
}
