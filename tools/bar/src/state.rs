use std::collections::BTreeMap;

use crate::config::FreshnessConfig;
use crate::{
    ActivityStatus, ActivityUpdate, BarSnapshot, CommandActivity, SourceHealth, SourceId,
    StateUpdate, SystemUpdate,
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
                let next_outputs = outputs
                    .into_iter()
                    .map(|output| (output.name.clone(), output))
                    .collect::<BTreeMap<_, _>>();
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
            StateUpdate::System(system_update) => self.apply_system_update(system_update),
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

    fn apply_system_update(&mut self, update: SystemUpdate) -> bool {
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
            SystemUpdate::Power(value) => {
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
                if self.snapshot.system.media == value {
                    false
                } else {
                    self.snapshot.system.media = value;
                    true
                }
            }
            SystemUpdate::Calendar(value) => {
                if self.snapshot.system.calendar == value {
                    false
                } else {
                    self.snapshot.system.calendar = value;
                    true
                }
            }
            SystemUpdate::Timers(value) => {
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
            SystemUpdate::Power(_) => SourceId::Power,
            SystemUpdate::Clock(_) => SourceId::Clock,
            SystemUpdate::Media(_) => SourceId::Media,
            SystemUpdate::Calendar(_) => SourceId::Calendar,
            SystemUpdate::Timers(_) => SourceId::Timers,
        },
        StateUpdate::Activity(_) => SourceId::Activity,
        StateUpdate::Health { source, .. } => *source,
    }
}

fn freshness_seconds(freshness: &FreshnessConfig, source: SourceId) -> Option<u64> {
    match source {
        SourceId::Compositor => Some(freshness.compositor_seconds),
        SourceId::Power => Some(freshness.power_seconds),
        SourceId::Resources => Some(freshness.resources_seconds),
        SourceId::Network => Some(freshness.network_seconds),
        SourceId::Bluetooth | SourceId::Audio => Some(freshness.bluetooth_seconds),
        SourceId::Media => Some(freshness.media_seconds),
        SourceId::Calendar => Some(freshness.calendar_seconds),
        SourceId::Timers => Some(freshness.timers_seconds),
        SourceId::Activity => Some(freshness.activity_seconds),
        SourceId::Clock => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CalendarEvent, SourceHealth, SourceId, StateUpdate, SystemUpdate, config::FreshnessConfig,
    };

    use super::StateStore;

    #[test]
    fn equivalent_updates_return_false_but_refresh_freshness() {
        let mut store = StateStore::new(FreshnessConfig::default());
        let update = StateUpdate::System(SystemUpdate::Calendar(Some(CalendarEvent {
            id: "review".to_string(),
            title: "Review".to_string(),
            location: Some("Room 1".to_string()),
            start_epoch: 1_800_000_300,
            end_epoch: Some(1_800_000_900),
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
    fn expire_keeps_compositor_state_visible_while_marking_it_stale() {
        let mut store = StateStore::new(FreshnessConfig::default());

        assert!(store.apply(
            StateUpdate::FocusedOutput(Some("DP-5".to_string())),
            1_800_000_000
        ));
        assert!(store.expire(1_800_000_011));
        assert_eq!(store.snapshot().focused_output.as_deref(), Some("DP-5"));
        assert_eq!(
            store
                .snapshot()
                .system
                .source_health
                .get(&SourceId::Compositor),
            Some(&SourceHealth::Stale {
                since_epoch: 1_800_000_010,
            })
        );
    }
}
