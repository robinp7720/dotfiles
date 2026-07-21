use std::collections::BTreeMap;

use crate::config::FreshnessConfig;
use crate::{
    ActivityStatus, ActivityUpdate, BarSnapshot, CalendarEvent, CommandActivity, MediaState,
    OutputState, PowerState, SourceHealth, SourceId, StateUpdate, SystemUpdate, TimerState,
    WindowState, WorkspaceState,
};

#[derive(Clone, Debug)]
pub struct StateStore {
    snapshot: BarSnapshot,
    freshness: FreshnessConfig,
    observed_at: BTreeMap<SourceId, i64>,
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new(FreshnessConfig::default())
    }
}

impl StateStore {
    pub fn new(freshness: FreshnessConfig) -> Self {
        Self {
            snapshot: BarSnapshot::default(),
            freshness,
            observed_at: BTreeMap::new(),
        }
    }

    pub fn snapshot(&self) -> &BarSnapshot {
        &self.snapshot
    }

    pub fn apply(&mut self, update: StateUpdate, observed_at: i64) -> bool {
        let source = source_for_update(&update);
        self.observed_at.insert(source, observed_at);

        let refreshes_health = !matches!(&update, StateUpdate::Health { .. });
        let mut dirty = match update {
            StateUpdate::Outputs(outputs) => {
                let next_outputs = normalize_outputs(outputs, &self.snapshot.outputs, observed_at);
                if self.snapshot.outputs == next_outputs {
                    false
                } else {
                    self.snapshot.outputs = next_outputs;
                    true
                }
            }
            StateUpdate::FocusedOutput(focused_output) => {
                if self.snapshot.focused_output == focused_output {
                    false
                } else {
                    self.snapshot.focused_output = focused_output;
                    true
                }
            }
            StateUpdate::System(system_update) => {
                self.apply_system_update(system_update, observed_at)
            }
            StateUpdate::Activity(activity_update) => self.apply_activity_update(activity_update),
            StateUpdate::Health { source, health } => {
                self.observed_at.insert(source, observed_at);
                self.set_source_health(source, health)
            }
        };

        if refreshes_health && !matches!(source, SourceId::Clock) {
            dirty |= self.set_source_health(source, SourceHealth::Healthy);
        }

        dirty
    }

    pub fn expire(&mut self, now_epoch: i64) -> bool {
        let mut dirty = false;

        for (source, seen_at) in self.observed_at.clone() {
            let Some(freshness_seconds) = freshness_seconds(&self.freshness, source) else {
                continue;
            };
            let stale_since = seen_at.saturating_add(freshness_seconds as i64);
            if now_epoch <= stale_since {
                continue;
            }

            if !matches!(
                self.snapshot.system.source_health.get(&source),
                Some(SourceHealth::Disconnected { .. })
            ) {
                dirty |= self.set_source_health(
                    source,
                    SourceHealth::Stale {
                        since_epoch: stale_since,
                    },
                );
            }

            dirty |= self.clear_stale_value(source);
        }

        dirty
    }

    fn apply_system_update(&mut self, update: SystemUpdate, observed_at: i64) -> bool {
        match update {
            SystemUpdate::KeyboardLayout(value) => {
                if self.snapshot.system.keyboard_layout == value {
                    false
                } else {
                    self.snapshot.system.keyboard_layout = value;
                    true
                }
            }
            SystemUpdate::Resources(value) => {
                if self.snapshot.system.resources == value {
                    false
                } else {
                    self.snapshot.system.resources = value;
                    true
                }
            }
            SystemUpdate::Network(value) => {
                if self.snapshot.system.network == value {
                    false
                } else {
                    self.snapshot.system.network = value;
                    true
                }
            }
            SystemUpdate::Bluetooth(value) => {
                if self.snapshot.system.bluetooth == value {
                    false
                } else {
                    self.snapshot.system.bluetooth = value;
                    true
                }
            }
            SystemUpdate::Audio(value) => {
                if self.snapshot.system.audio == value {
                    false
                } else {
                    self.snapshot.system.audio = value;
                    true
                }
            }
            SystemUpdate::Brightness(value) => {
                if self.snapshot.system.brightness == value {
                    false
                } else {
                    self.snapshot.system.brightness = value;
                    true
                }
            }
            SystemUpdate::Power(value) => {
                let value = normalize_power(value, &self.snapshot.system.power, observed_at);
                if self.snapshot.system.power == value {
                    false
                } else {
                    self.snapshot.system.power = value;
                    true
                }
            }
            SystemUpdate::Clock(value) => {
                if self.snapshot.system.clock == value {
                    false
                } else {
                    self.snapshot.system.clock = value;
                    true
                }
            }
            SystemUpdate::Media(value) => {
                let value =
                    normalize_media(value, self.snapshot.system.media.as_ref(), observed_at);
                if self.snapshot.system.media == value {
                    false
                } else {
                    self.snapshot.system.media = value;
                    true
                }
            }
            SystemUpdate::Calendar(value) => {
                let value =
                    normalize_calendar(value, self.snapshot.system.calendar.as_ref(), observed_at);
                if self.snapshot.system.calendar == value {
                    false
                } else {
                    self.snapshot.system.calendar = value;
                    true
                }
            }
            SystemUpdate::CalendarAgenda(value) => {
                if self.snapshot.system.calendar_agenda == value {
                    false
                } else {
                    self.snapshot.system.calendar_agenda = value;
                    true
                }
            }
            SystemUpdate::Timers(value) => {
                let value = normalize_timers(value, &self.snapshot.system.timers, observed_at);
                if self.snapshot.system.timers == value {
                    false
                } else {
                    self.snapshot.system.timers = value;
                    true
                }
            }
        }
    }

    fn apply_activity_update(&mut self, update: ActivityUpdate) -> bool {
        match update {
            ActivityUpdate::Started(activity) => insert_activity(&mut self.snapshot, activity),
            ActivityUpdate::Finished {
                id,
                finished_at,
                exit_code,
            } => finish_activity(&mut self.snapshot, &id, finished_at, exit_code),
            ActivityUpdate::Snapshot(activities) => {
                let next_items = activities
                    .into_iter()
                    .map(|activity| (activity.id.clone(), activity))
                    .collect::<BTreeMap<_, _>>();
                if self.snapshot.activities.items == next_items {
                    false
                } else {
                    self.snapshot.activities.items = next_items;
                    true
                }
            }
            ActivityUpdate::Removed { id } => self.snapshot.activities.items.remove(&id).is_some(),
        }
    }

    fn clear_stale_value(&mut self, source: SourceId) -> bool {
        match source {
            SourceId::Calendar => {
                if self.snapshot.system.calendar.is_some() {
                    self.snapshot.system.calendar = None;
                    true
                } else {
                    false
                }
            }
            SourceId::CalendarAgenda => {
                if self.snapshot.system.calendar_agenda.is_some() {
                    self.snapshot.system.calendar_agenda = None;
                    true
                } else {
                    false
                }
            }
            SourceId::Media => {
                if self.snapshot.system.media.is_some() {
                    self.snapshot.system.media = None;
                    true
                } else {
                    false
                }
            }
            SourceId::Timers => {
                if self.snapshot.system.timers.is_empty() {
                    false
                } else {
                    self.snapshot.system.timers.clear();
                    true
                }
            }
            SourceId::Activity => {
                if self.snapshot.activities.items.is_empty() {
                    false
                } else {
                    self.snapshot.activities.items.clear();
                    true
                }
            }
            SourceId::Brightness => {
                if self.snapshot.system.brightness == crate::BrightnessState::default() {
                    false
                } else {
                    self.snapshot.system.brightness = crate::BrightnessState::default();
                    true
                }
            }
            _ => false,
        }
    }

    fn set_source_health(&mut self, source: SourceId, health: SourceHealth) -> bool {
        if self.snapshot.system.source_health.get(&source) == Some(&health) {
            false
        } else {
            self.snapshot.system.source_health.insert(source, health);
            true
        }
    }
}

fn normalize_outputs(
    outputs: Vec<OutputState>,
    current: &BTreeMap<String, OutputState>,
    observed_at: i64,
) -> BTreeMap<String, OutputState> {
    outputs
        .into_iter()
        .map(|mut output| {
            let previous = current.get(&output.name);
            output.changed_at = previous
                .filter(|previous| output_semantically_equal(&output, previous))
                .map_or(observed_at, |previous| previous.changed_at);

            for workspace in &mut output.workspaces {
                let previous_workspace = previous.and_then(|previous| {
                    previous
                        .workspaces
                        .iter()
                        .find(|candidate| candidate.id == workspace.id)
                });
                workspace.changed_at = previous_workspace
                    .filter(|previous| workspace_semantically_equal(workspace, previous))
                    .map_or(observed_at, |previous| previous.changed_at);
            }

            for window in &mut output.windows {
                let previous_window = previous.and_then(|previous| {
                    previous
                        .windows
                        .iter()
                        .find(|candidate| candidate.id == window.id)
                });
                window.changed_at = previous_window
                    .filter(|previous| window_semantically_equal(window, previous))
                    .map_or(observed_at, |previous| previous.changed_at);
            }

            if let Some(window) = output.focused_window.as_mut() {
                let previous_window =
                    previous.and_then(|previous| previous.focused_window.as_ref());
                window.changed_at = previous_window
                    .filter(|previous| window_semantically_equal(window, previous))
                    .map_or(observed_at, |previous| previous.changed_at);
            }

            (output.name.clone(), output)
        })
        .collect()
}

fn normalize_power(mut value: PowerState, current: &PowerState, observed_at: i64) -> PowerState {
    value.changed_at = if power_semantically_equal(&value, current) {
        current.changed_at
    } else {
        observed_at
    };
    value
}

fn normalize_media(
    value: Option<MediaState>,
    current: Option<&MediaState>,
    observed_at: i64,
) -> Option<MediaState> {
    value.map(|mut value| {
        value.changed_at = current
            .filter(|current| media_semantically_equal(&value, current))
            .map_or(observed_at, |current| current.changed_at);
        value
    })
}

fn normalize_calendar(
    value: Option<CalendarEvent>,
    current: Option<&CalendarEvent>,
    observed_at: i64,
) -> Option<CalendarEvent> {
    value.map(|mut value| {
        value.changed_at = current
            .filter(|current| calendar_semantically_equal(&value, current))
            .map_or(observed_at, |current| current.changed_at);
        value
    })
}

fn normalize_timers(
    timers: Vec<TimerState>,
    current: &[TimerState],
    observed_at: i64,
) -> Vec<TimerState> {
    timers
        .into_iter()
        .map(|mut timer| {
            timer.changed_at = current
                .iter()
                .find(|candidate| candidate.id == timer.id)
                .filter(|current| timer_semantically_equal(&timer, current))
                .map_or(observed_at, |current| current.changed_at);
            timer
        })
        .collect()
}

fn output_semantically_equal(left: &OutputState, right: &OutputState) -> bool {
    left.name == right.name
        && left.urgent == right.urgent
        && left.workspaces.len() == right.workspaces.len()
        && left
            .workspaces
            .iter()
            .zip(&right.workspaces)
            .all(|(left, right)| workspace_semantically_equal(left, right))
        && left.windows.len() == right.windows.len()
        && left
            .windows
            .iter()
            .zip(&right.windows)
            .all(|(left, right)| window_semantically_equal(left, right))
        && match (&left.focused_window, &right.focused_window) {
            (Some(left), Some(right)) => window_semantically_equal(left, right),
            (None, None) => true,
            _ => false,
        }
}

fn workspace_semantically_equal(left: &WorkspaceState, right: &WorkspaceState) -> bool {
    left.id == right.id
        && left.label == right.label
        && left.output == right.output
        && left.active == right.active
        && left.urgent == right.urgent
}

fn window_semantically_equal(left: &WindowState, right: &WindowState) -> bool {
    left.id == right.id
        && left.app_id == right.app_id
        && left.title == right.title
        && left.urgent == right.urgent
        && left.workspace_id == right.workspace_id
}

fn power_semantically_equal(left: &PowerState, right: &PowerState) -> bool {
    left.battery_present == right.battery_present
        && left.battery_percent == right.battery_percent
        && left.charging == right.charging
        && left.profile == right.profile
}

fn media_semantically_equal(left: &MediaState, right: &MediaState) -> bool {
    left.player == right.player
        && left.status == right.status
        && left.title == right.title
        && left.artist == right.artist
        && left.art_url == right.art_url
}

fn calendar_semantically_equal(left: &CalendarEvent, right: &CalendarEvent) -> bool {
    left.id == right.id
        && left.title == right.title
        && left.location == right.location
        && left.start_epoch == right.start_epoch
        && left.end_epoch == right.end_epoch
}

fn timer_semantically_equal(left: &TimerState, right: &TimerState) -> bool {
    left.id == right.id
        && left.label == right.label
        && left.remaining_seconds == right.remaining_seconds
        && left.target_epoch == right.target_epoch
        && left.completed == right.completed
}

fn insert_activity(snapshot: &mut BarSnapshot, activity: CommandActivity) -> bool {
    if snapshot.activities.items.get(&activity.id) == Some(&activity) {
        false
    } else {
        snapshot
            .activities
            .items
            .insert(activity.id.clone(), activity);
        true
    }
}

fn finish_activity(snapshot: &mut BarSnapshot, id: &str, finished_at: i64, exit_code: i32) -> bool {
    let Some(existing) = snapshot.activities.items.get_mut(id) else {
        return false;
    };
    let next_status = if exit_code == 0 {
        ActivityStatus::Succeeded
    } else {
        ActivityStatus::Failed
    };

    let changed = existing.status != next_status
        || existing.finished_at != Some(finished_at)
        || existing.exit_code != Some(exit_code);
    if changed {
        existing.status = next_status;
        existing.finished_at = Some(finished_at);
        existing.exit_code = Some(exit_code);
    }
    changed
}

fn source_for_update(update: &StateUpdate) -> SourceId {
    match update {
        StateUpdate::Outputs(_) | StateUpdate::FocusedOutput(_) => SourceId::Compositor,
        StateUpdate::System(update) => match update {
            SystemUpdate::KeyboardLayout(_) => SourceId::Compositor,
            SystemUpdate::Resources(_) => SourceId::Resources,
            SystemUpdate::Network(_) => SourceId::Network,
            SystemUpdate::Bluetooth(_) => SourceId::Bluetooth,
            SystemUpdate::Audio(_) => SourceId::Audio,
            SystemUpdate::Brightness(_) => SourceId::Brightness,
            SystemUpdate::Power(_) => SourceId::Power,
            SystemUpdate::Clock(_) => SourceId::Clock,
            SystemUpdate::Media(_) => SourceId::Media,
            SystemUpdate::Calendar(_) => SourceId::Calendar,
            SystemUpdate::CalendarAgenda(_) => SourceId::CalendarAgenda,
            SystemUpdate::Timers(_) => SourceId::Timers,
        },
        StateUpdate::Activity(_) => SourceId::Activity,
        StateUpdate::Health { source, .. } => *source,
    }
}

fn freshness_seconds(freshness: &FreshnessConfig, source: SourceId) -> Option<u64> {
    match source {
        // Connected event streams own their health lifecycle and explicitly publish
        // disconnects. A quiet stream is not stale: a song can keep playing, an
        // urgent window can remain urgent, and a connection can remain unchanged.
        SourceId::Compositor
        | SourceId::Network
        | SourceId::Bluetooth
        | SourceId::Audio
        | SourceId::Media
        | SourceId::Activity => None,
        SourceId::Power => Some(freshness.power_seconds),
        SourceId::Resources => Some(freshness.resources_seconds),
        SourceId::Brightness => Some(freshness.brightness_seconds),
        SourceId::Calendar | SourceId::CalendarAgenda => Some(freshness.calendar_seconds),
        SourceId::Timers => Some(freshness.timers_seconds),
        SourceId::Clock => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        BrightnessState, CalendarEvent, MediaState, OutputState, PlaybackStatus, PowerProfile,
        PowerState, SourceHealth, SourceId, StateUpdate, SystemUpdate, TimerState, WindowState,
        WorkspaceState, config::FreshnessConfig,
    };

    use super::StateStore;

    #[test]
    fn stale_brightness_is_hidden_after_its_freshness_window() {
        let mut store = StateStore::new(FreshnessConfig::default());
        let brightness = BrightnessState {
            device: Some("intel_backlight".to_string()),
            percent: Some(70),
        };

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Brightness(brightness)),
            1_800_000_000,
        ));
        assert!(store.expire(1_800_000_011));
        assert_eq!(
            store.snapshot().system.brightness,
            BrightnessState::default()
        );
        assert!(matches!(
            store
                .snapshot()
                .system
                .source_health
                .get(&SourceId::Brightness),
            Some(SourceHealth::Stale { .. })
        ));
    }

    #[test]
    fn equivalent_updates_return_false_but_refresh_freshness() {
        let mut store = StateStore::new(FreshnessConfig::default());
        let update = StateUpdate::System(SystemUpdate::Calendar(Some(CalendarEvent {
            id: "review".to_string(),
            title: "Review".to_string(),
            location: Some("Room 1".to_string()),
            start_epoch: 1_800_000_300,
            end_epoch: Some(1_800_000_900),
            changed_at: 0,
        })));

        assert!(store.apply(update.clone(), 1_800_000_000));
        assert!(!store.apply(update, 1_800_000_030));
        assert!(!store.expire(1_800_000_080));
        assert_eq!(
            store
                .snapshot()
                .system
                .source_health
                .get(&SourceId::Calendar),
            Some(&SourceHealth::Healthy)
        );
        assert!(store.snapshot().system.calendar.is_some());
    }

    #[test]
    fn expire_marks_calendar_stale_and_hides_the_event() {
        let mut store = StateStore::new(FreshnessConfig::default());

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Calendar(Some(CalendarEvent {
                id: "review".to_string(),
                title: "Review".to_string(),
                location: Some("Room 1".to_string()),
                start_epoch: 1_800_000_300,
                end_epoch: Some(1_800_000_900),
                changed_at: 0,
            }))),
            1_800_000_000,
        ));
        assert!(store.expire(1_800_000_061));
        assert_eq!(store.snapshot().system.calendar, None);
        assert_eq!(
            store
                .snapshot()
                .system
                .source_health
                .get(&SourceId::Calendar),
            Some(&SourceHealth::Stale {
                since_epoch: 1_800_000_060,
            })
        );
    }

    #[test]
    fn quiet_event_streams_remain_healthy_without_periodic_state_changes() {
        let mut store = StateStore::new(FreshnessConfig::default());

        assert!(store.apply(
            StateUpdate::FocusedOutput(Some("DP-5".to_string())),
            1_800_000_000
        ));
        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Media(Some(MediaState {
                player: "player".to_string(),
                status: PlaybackStatus::Playing,
                title: Some("Track".to_string()),
                artist: Some("Artist".to_string()),
                art_url: None,
                changed_at: 0,
            }))),
            1_800_000_000,
        ));

        assert!(!store.expire(1_800_001_000));
        assert_eq!(store.snapshot().focused_output.as_deref(), Some("DP-5"));
        assert!(store.snapshot().system.media.is_some());
        assert_eq!(
            store
                .snapshot()
                .system
                .source_health
                .get(&SourceId::Compositor),
            Some(&SourceHealth::Healthy)
        );
        assert_eq!(
            store.snapshot().system.source_health.get(&SourceId::Media),
            Some(&SourceHealth::Healthy)
        );
    }

    #[test]
    fn artwork_uri_changes_refresh_media_state() {
        let mut store = StateStore::new(FreshnessConfig::default());
        let media = MediaState {
            player: "player".to_string(),
            status: PlaybackStatus::Playing,
            title: Some("Track".to_string()),
            artist: Some("Artist".to_string()),
            art_url: Some("https://example.test/first.jpg".to_string()),
            changed_at: 0,
        };

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Media(Some(media.clone()))),
            1_800_000_000,
        ));
        let mut changed = media;
        changed.art_url = Some("https://example.test/second.jpg".to_string());
        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Media(Some(changed))),
            1_800_000_010,
        ));
        assert_eq!(
            store.snapshot().system.media.as_ref().unwrap().changed_at,
            1_800_000_010
        );
    }

    #[test]
    fn meaningful_calendar_changes_receive_observation_timestamp() {
        let mut store = StateStore::new(FreshnessConfig::default());
        let event = CalendarEvent {
            id: "review".to_string(),
            title: "Review".to_string(),
            location: Some("Room 1".to_string()),
            start_epoch: 1_800_000_300,
            end_epoch: Some(1_800_000_900),
            changed_at: 0,
        };

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Calendar(Some(event.clone()))),
            1_800_000_000,
        ));
        assert_eq!(
            store
                .snapshot()
                .system
                .calendar
                .as_ref()
                .unwrap()
                .changed_at,
            1_800_000_000
        );

        assert!(!store.apply(
            StateUpdate::System(SystemUpdate::Calendar(Some(event.clone()))),
            1_800_000_030,
        ));
        assert_eq!(
            store
                .snapshot()
                .system
                .calendar
                .as_ref()
                .unwrap()
                .changed_at,
            1_800_000_000
        );

        let mut changed = event;
        changed.title = "Updated review".to_string();
        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Calendar(Some(changed))),
            1_800_000_040,
        ));
        assert_eq!(
            store
                .snapshot()
                .system
                .calendar
                .as_ref()
                .unwrap()
                .changed_at,
            1_800_000_040
        );
    }

    #[test]
    fn context_sources_receive_observation_timestamps() {
        let mut store = StateStore::new(FreshnessConfig::default());

        assert!(store.apply(
            StateUpdate::Outputs(vec![OutputState {
                name: "DP-5".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "1".to_string(),
                    label: "1".to_string(),
                    output: "DP-5".to_string(),
                    active: true,
                    urgent: true,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "window-42".to_string(),
                    app_id: Some("terminal".to_string()),
                    title: "Build failed".to_string(),
                    urgent: true,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "window-42".to_string(),
                    app_id: Some("terminal".to_string()),
                    title: "Build failed".to_string(),
                    urgent: true,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: true,
                changed_at: 0,
            }]),
            1_800_000_010,
        ));
        let output = store.snapshot().outputs.get("DP-5").unwrap();
        assert_eq!(output.changed_at, 1_800_000_010);
        assert_eq!(output.workspaces[0].changed_at, 1_800_000_010);
        assert_eq!(
            output.focused_window.as_ref().unwrap().changed_at,
            1_800_000_010
        );

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Power(PowerState {
                battery_present: true,
                battery_percent: Some(6),
                charging: false,
                profile: PowerProfile::Balanced,
                changed_at: 0,
            })),
            1_800_000_020,
        ));
        assert_eq!(store.snapshot().system.power.changed_at, 1_800_000_020);

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Media(Some(MediaState {
                player: "player".to_string(),
                status: PlaybackStatus::Playing,
                title: Some("Track".to_string()),
                artist: Some("Artist".to_string()),
                art_url: None,
                changed_at: 0,
            }))),
            1_800_000_030,
        ));
        assert_eq!(
            store.snapshot().system.media.as_ref().unwrap().changed_at,
            1_800_000_030
        );

        assert!(store.apply(
            StateUpdate::System(SystemUpdate::Timers(vec![TimerState {
                id: "tea".to_string(),
                label: "Tea".to_string(),
                remaining_seconds: 60,
                target_epoch: Some(1_800_000_100),
                completed: false,
                changed_at: 0,
            }])),
            1_800_000_040,
        ));
        assert_eq!(store.snapshot().system.timers[0].changed_at, 1_800_000_040);
    }
}
