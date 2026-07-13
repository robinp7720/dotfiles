use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{
    ActivityStatus, BarSnapshot, CalendarEvent, CommandActivity, MediaState, PlaybackStatus,
    PowerState, SourceHealth, SourceId, ThresholdConfig, TimerState,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextTier {
    Ambient = 0,
    Work = 1,
    Imminent = 2,
    Critical = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextCard {
    Battery {
        percent: u8,
        charging: bool,
        tier: ContextTier,
    },
    Calendar {
        id: String,
        title: String,
        location: Option<String>,
        start_epoch: i64,
        tier: ContextTier,
    },
    Timer {
        id: String,
        label: String,
        remaining_seconds: u64,
        target_epoch: Option<i64>,
        completed: bool,
        tier: ContextTier,
    },
    Activity {
        id: String,
        label: String,
        cwd: PathBuf,
        status: ActivityStatus,
        started_at: i64,
        finished_at: Option<i64>,
    },
    Media {
        player: String,
        status: PlaybackStatus,
        title: Option<String>,
        artist: Option<String>,
    },
    Urgent {
        output: String,
        workspace: Option<String>,
        window_id: Option<String>,
        window_title: Option<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dismissals {
    entries: BTreeMap<String, DismissalEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DismissalEntry {
    generation: String,
    until_epoch: Option<i64>,
}

#[derive(Clone, Debug)]
struct Candidate {
    card: ContextCard,
    key: String,
    generation: String,
    tier: ContextTier,
    action_deadline: Option<i64>,
    changed_at: i64,
}

fn priority_key(candidate: &Candidate) -> (u8, Reverse<i64>, i64) {
    (
        candidate.tier as u8,
        Reverse(candidate.action_deadline.unwrap_or(i64::MAX)),
        candidate.changed_at,
    )
}

pub fn select_context(
    snapshot: &BarSnapshot,
    now_epoch: i64,
    thresholds: &ThresholdConfig,
    dismissals: &Dismissals,
) -> Option<ContextCard> {
    candidates(snapshot, now_epoch, thresholds)
        .into_iter()
        .filter(|candidate| !dismissals.suppresses(candidate, now_epoch))
        .max_by_key(priority_key)
        .map(|candidate| candidate.card)
}

impl Dismissals {
    pub fn dismiss(&mut self, card: &ContextCard, now_epoch: i64, critical_snooze_seconds: u64) {
        let (key, generation) = dismissal_identity(card);
        let until_epoch = is_snoozable_critical(card).then(|| {
            now_epoch.saturating_add(i64::try_from(critical_snooze_seconds).unwrap_or(i64::MAX))
        });
        self.entries.insert(
            key,
            DismissalEntry {
                generation,
                until_epoch,
            },
        );
    }

    fn suppresses(&self, candidate: &Candidate, now_epoch: i64) -> bool {
        self.entries
            .get(&candidate.key)
            .is_some_and(|entry| entry.generation == candidate.generation)
            && self
                .entries
                .get(&candidate.key)
                .and_then(|entry| entry.until_epoch)
                .map_or(true, |until_epoch| now_epoch < until_epoch)
    }
}

fn is_snoozable_critical(card: &ContextCard) -> bool {
    matches!(
        card,
        ContextCard::Battery {
            tier: ContextTier::Critical,
            ..
        } | ContextCard::Timer {
            completed: true,
            ..
        }
    )
}

fn candidates(
    snapshot: &BarSnapshot,
    now_epoch: i64,
    thresholds: &ThresholdConfig,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    if source_is_healthy(snapshot, SourceId::Compositor) {
        candidates.extend(urgent_candidates(snapshot));
    }

    if source_is_healthy(snapshot, SourceId::Power) {
        candidates.extend(battery_candidates(&snapshot.system.power, thresholds));
    }

    if source_is_healthy(snapshot, SourceId::Timers) {
        candidates.extend(timer_candidates(
            &snapshot.system.timers,
            now_epoch,
            thresholds,
        ));
    }

    if source_is_healthy(snapshot, SourceId::Calendar) {
        if let Some(calendar) = snapshot.system.calendar.as_ref() {
            candidates.push(calendar_candidate(calendar, now_epoch, thresholds));
        }
    }

    if source_is_healthy(snapshot, SourceId::Activity) {
        candidates.extend(activity_candidates(snapshot, now_epoch, thresholds));
    }

    if source_is_healthy(snapshot, SourceId::Media) {
        if let Some(media) = snapshot.system.media.as_ref() {
            candidates.push(media_candidate(media));
        }
    }

    candidates
}

fn urgent_candidates(snapshot: &BarSnapshot) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    for (output_name, output) in &snapshot.outputs {
        if output.urgent {
            candidates.push(Candidate {
                card: ContextCard::Urgent {
                    output: output_name.clone(),
                    workspace: None,
                    window_id: None,
                    window_title: output
                        .focused_window
                        .as_ref()
                        .map(|window| window.title.clone()),
                },
                key: urgent_identity(output_name, None, None),
                generation: "urgent".to_string(),
                tier: ContextTier::Critical,
                action_deadline: None,
                changed_at: output.changed_at,
            });
        }

        for workspace in output
            .workspaces
            .iter()
            .filter(|workspace| workspace.urgent)
        {
            candidates.push(Candidate {
                card: ContextCard::Urgent {
                    output: output_name.clone(),
                    workspace: Some(workspace.id.clone()),
                    window_id: None,
                    window_title: output
                        .focused_window
                        .as_ref()
                        .map(|window| window.title.clone()),
                },
                key: urgent_identity(output_name, Some(&workspace.id), None),
                generation: "urgent".to_string(),
                tier: ContextTier::Critical,
                action_deadline: None,
                changed_at: workspace.changed_at,
            });
        }

        if let Some(window) = output
            .focused_window
            .as_ref()
            .filter(|window| window.urgent)
        {
            candidates.push(Candidate {
                card: ContextCard::Urgent {
                    output: output_name.clone(),
                    workspace: None,
                    window_id: Some(window.id.clone()),
                    window_title: Some(window.title.clone()),
                },
                key: urgent_identity(output_name, None, Some(&window.id)),
                generation: "urgent".to_string(),
                tier: ContextTier::Critical,
                action_deadline: None,
                changed_at: window.changed_at,
            });
        }
    }

    candidates
}

fn battery_candidates(power: &PowerState, thresholds: &ThresholdConfig) -> Vec<Candidate> {
    let Some(percent) = power.battery_percent else {
        return Vec::new();
    };

    if power.charging {
        return Vec::new();
    }

    if percent <= thresholds.battery_critical_percent {
        return vec![Candidate {
            card: ContextCard::Battery {
                percent,
                charging: power.charging,
                tier: ContextTier::Critical,
            },
            key: "battery".to_string(),
            generation: "critical".to_string(),
            tier: ContextTier::Critical,
            action_deadline: None,
            changed_at: power.changed_at,
        }];
    }

    if percent <= thresholds.battery_low_percent {
        return vec![Candidate {
            card: ContextCard::Battery {
                percent,
                charging: power.charging,
                tier: ContextTier::Imminent,
            },
            key: "battery".to_string(),
            generation: "low".to_string(),
            tier: ContextTier::Imminent,
            action_deadline: None,
            changed_at: power.changed_at,
        }];
    }

    Vec::new()
}

fn timer_candidates(
    timers: &[TimerState],
    now_epoch: i64,
    thresholds: &ThresholdConfig,
) -> Vec<Candidate> {
    let soon_seconds = thresholds.timer_soon_minutes.saturating_mul(60);

    timers
        .iter()
        .filter_map(|timer| {
            if timer.completed {
                return Some(Candidate {
                    card: ContextCard::Timer {
                        id: timer.id.clone(),
                        label: timer.label.clone(),
                        remaining_seconds: timer.remaining_seconds,
                        target_epoch: timer.target_epoch,
                        completed: true,
                        tier: ContextTier::Critical,
                    },
                    key: format!("timer:{}", timer.id),
                    generation: "completed".to_string(),
                    tier: ContextTier::Critical,
                    action_deadline: timer.target_epoch.or(Some(now_epoch)),
                    changed_at: timer.changed_at,
                });
            }

            let deadline = timer
                .target_epoch
                .unwrap_or_else(|| now_epoch.saturating_add(timer.remaining_seconds as i64));

            if timer.remaining_seconds <= soon_seconds
                || deadline <= now_epoch + soon_seconds as i64
            {
                return Some(Candidate {
                    card: ContextCard::Timer {
                        id: timer.id.clone(),
                        label: timer.label.clone(),
                        remaining_seconds: timer.remaining_seconds,
                        target_epoch: timer.target_epoch,
                        completed: false,
                        tier: ContextTier::Imminent,
                    },
                    key: format!("timer:{}", timer.id),
                    generation: "imminent".to_string(),
                    tier: ContextTier::Imminent,
                    action_deadline: Some(deadline),
                    changed_at: timer.changed_at,
                });
            }

            None
        })
        .collect()
}

fn calendar_candidate(
    calendar: &CalendarEvent,
    now_epoch: i64,
    thresholds: &ThresholdConfig,
) -> Candidate {
    let imminent_threshold = thresholds.calendar_soon_minutes as i64 * 60;
    let tier = if calendar.start_epoch <= now_epoch + imminent_threshold {
        ContextTier::Imminent
    } else {
        ContextTier::Ambient
    };
    let generation = if tier == ContextTier::Imminent {
        "imminent"
    } else {
        "ambient"
    };

    Candidate {
        card: ContextCard::Calendar {
            id: calendar.id.clone(),
            title: calendar.title.clone(),
            location: calendar.location.clone(),
            start_epoch: calendar.start_epoch,
            tier,
        },
        key: format!("calendar:{}", calendar.id),
        generation: generation.to_string(),
        tier,
        action_deadline: Some(calendar.start_epoch),
        changed_at: calendar.changed_at,
    }
}

fn activity_candidates(
    snapshot: &BarSnapshot,
    now_epoch: i64,
    thresholds: &ThresholdConfig,
) -> Vec<Candidate> {
    let visible_for = thresholds.work_completed_seconds as i64;

    snapshot
        .activities
        .items
        .values()
        .filter_map(|activity| match activity.status {
            ActivityStatus::Running => Some(activity_candidate(
                activity,
                None,
                activity.started_at,
                "running",
            )),
            ActivityStatus::Succeeded | ActivityStatus::Failed => {
                let finished_at = activity.finished_at?;
                if finished_at + visible_for < now_epoch {
                    return None;
                }

                Some(activity_candidate(
                    activity,
                    Some(finished_at + visible_for),
                    finished_at,
                    match activity.status {
                        ActivityStatus::Succeeded => "succeeded",
                        ActivityStatus::Failed => "failed",
                        ActivityStatus::Running => unreachable!(),
                    },
                ))
            }
        })
        .collect()
}

fn activity_candidate(
    activity: &CommandActivity,
    action_deadline: Option<i64>,
    changed_at: i64,
    generation: &str,
) -> Candidate {
    Candidate {
        card: ContextCard::Activity {
            id: activity.id.clone(),
            label: activity.label.clone(),
            cwd: activity.cwd.clone(),
            status: activity.status.clone(),
            started_at: activity.started_at,
            finished_at: activity.finished_at,
        },
        key: format!("activity:{}", activity.id),
        generation: generation.to_string(),
        tier: ContextTier::Work,
        action_deadline,
        changed_at,
    }
}

fn media_candidate(media: &MediaState) -> Candidate {
    Candidate {
        card: ContextCard::Media {
            player: media.player.clone(),
            status: media.status.clone(),
            title: media.title.clone(),
            artist: media.artist.clone(),
        },
        key: format!("media:{}", media.player),
        generation: format!(
            "{}:{}:{}",
            media.player,
            media.title.as_deref().unwrap_or_default(),
            media.artist.as_deref().unwrap_or_default()
        ),
        tier: ContextTier::Ambient,
        action_deadline: None,
        changed_at: media.changed_at,
    }
}

fn dismissal_identity(card: &ContextCard) -> (String, String) {
    match card {
        ContextCard::Battery { tier, .. } => (
            "battery".to_string(),
            match tier {
                ContextTier::Critical => "critical",
                ContextTier::Imminent => "low",
                ContextTier::Work | ContextTier::Ambient => "other",
            }
            .to_string(),
        ),
        ContextCard::Calendar { id, tier, .. } => (
            format!("calendar:{id}"),
            match tier {
                ContextTier::Imminent => "imminent",
                ContextTier::Ambient => "ambient",
                ContextTier::Work | ContextTier::Critical => "other",
            }
            .to_string(),
        ),
        ContextCard::Timer {
            id,
            completed,
            tier,
            ..
        } => (
            format!("timer:{id}"),
            if *completed || *tier == ContextTier::Critical {
                "completed"
            } else {
                "imminent"
            }
            .to_string(),
        ),
        ContextCard::Activity { id, status, .. } => (
            format!("activity:{id}"),
            match status {
                ActivityStatus::Running => "running",
                ActivityStatus::Succeeded => "succeeded",
                ActivityStatus::Failed => "failed",
            }
            .to_string(),
        ),
        ContextCard::Media {
            player,
            title,
            artist,
            ..
        } => (
            format!("media:{player}"),
            format!(
                "{}:{}",
                title.as_deref().unwrap_or_default(),
                artist.as_deref().unwrap_or_default()
            ),
        ),
        ContextCard::Urgent {
            output,
            workspace,
            window_id,
            ..
        } => (
            urgent_identity(output, workspace.as_deref(), window_id.as_deref()),
            "urgent".to_string(),
        ),
    }
}

fn urgent_identity(output: &str, workspace: Option<&str>, window_id: Option<&str>) -> String {
    format!(
        "urgent:{}:{}:{}",
        output,
        workspace.unwrap_or_default(),
        window_id.unwrap_or_default()
    )
}

fn source_is_healthy(snapshot: &BarSnapshot, source: SourceId) -> bool {
    !matches!(
        snapshot.system.source_health.get(&source),
        Some(SourceHealth::Stale { .. } | SourceHealth::Disconnected { .. })
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::{
        ActivityState, ActivityStatus, BarSnapshot, CalendarEvent, CommandActivity, MediaState,
        OutputState, PlaybackStatus, PowerProfile, PowerState, ThresholdConfig, TimerState,
        WindowState,
    };

    use super::{ContextCard, Dismissals, select_context};

    #[test]
    fn critical_battery_overrides_work_and_calendar() {
        let now = 1_800_000_000;
        let snapshot = fixture_snapshot()
            .with_build("cargo test", ActivityStatus::Running)
            .with_event("review", now + 10 * 60)
            .with_battery(6, false)
            .build();

        assert!(matches!(
            select_context(
                &snapshot,
                now,
                &ThresholdConfig::default(),
                &Dismissals::default()
            ),
            Some(ContextCard::Battery { percent: 6, .. })
        ));
    }

    #[test]
    fn imminent_calendar_beats_running_build() {
        let now = 1_800_000_000;
        let snapshot = fixture_snapshot()
            .with_build("cargo test", ActivityStatus::Running)
            .with_event("review", now + 10 * 60)
            .build();

        assert!(matches!(
            select_context(&snapshot, now, &ThresholdConfig::default(), &Dismissals::default()),
            Some(ContextCard::Calendar { ref id, .. }) if id == "review"
        ));
    }

    #[test]
    fn completed_timer_beats_calendar_and_work() {
        let now = 1_800_000_000;
        let snapshot = fixture_snapshot()
            .with_build("cargo test", ActivityStatus::Running)
            .with_event("review", now + 10 * 60)
            .with_timer("tea", 0, Some(now), true)
            .build();

        assert!(matches!(
            select_context(&snapshot, now, &ThresholdConfig::default(), &Dismissals::default()),
            Some(ContextCard::Timer { ref id, completed: true, .. }) if id == "tea"
        ));
    }

    #[test]
    fn dismissal_holds_until_severity_changes() {
        let now = 1_800_000_000;
        let thresholds = ThresholdConfig::default();
        let mut dismissals = Dismissals::default();
        let imminent_timer = fixture_snapshot()
            .with_timer("tea", 5 * 60, Some(now + 5 * 60), false)
            .build();
        let selected = select_context(&imminent_timer, now, &thresholds, &dismissals)
            .expect("expected imminent timer");

        dismissals.dismiss(&selected, now, thresholds.critical_snooze_seconds);

        assert_eq!(
            select_context(&imminent_timer, now, &thresholds, &dismissals),
            None
        );

        let completed_timer = fixture_snapshot()
            .with_timer("tea", 0, Some(now), true)
            .build();

        assert!(matches!(
            select_context(&completed_timer, now, &thresholds, &dismissals),
            Some(ContextCard::Timer { ref id, completed: true, .. }) if id == "tea"
        ));
    }

    #[test]
    fn dismissing_an_override_restores_the_previous_card() {
        let now = 1_800_000_000;
        let thresholds = ThresholdConfig::default();
        let mut dismissals = Dismissals::default();
        let snapshot = fixture_snapshot()
            .with_build("cargo test", ActivityStatus::Running)
            .with_event("review", now + 10 * 60)
            .build();
        let selected = select_context(&snapshot, now, &thresholds, &dismissals)
            .expect("expected calendar override");

        dismissals.dismiss(&selected, now, thresholds.critical_snooze_seconds);

        assert!(matches!(
            select_context(&snapshot, now, &thresholds, &dismissals),
            Some(ContextCard::Activity { ref id, .. }) if id == "cargo test"
        ));
    }

    #[test]
    fn urgent_window_dismissal_uses_stable_window_identity() {
        let now = 1_800_000_000;
        let thresholds = ThresholdConfig::default();
        let mut dismissals = Dismissals::default();
        let mut snapshot = fixture_snapshot()
            .with_urgent_window("window-42", "Build failed")
            .build();
        let selected = select_context(&snapshot, now, &thresholds, &dismissals)
            .expect("expected urgent window");

        dismissals.dismiss(&selected, now, thresholds.critical_snooze_seconds);
        snapshot
            .outputs
            .get_mut("DP-5")
            .and_then(|output| output.focused_window.as_mut())
            .expect("expected focused window")
            .title = "Build failed again".to_string();

        assert_eq!(
            select_context(&snapshot, now, &thresholds, &dismissals),
            None
        );

        snapshot
            .outputs
            .get_mut("DP-5")
            .and_then(|output| output.focused_window.as_mut())
            .expect("expected focused window")
            .id = "window-43".to_string();

        assert!(matches!(
            select_context(&snapshot, now, &thresholds, &dismissals),
            Some(ContextCard::Urgent { .. })
        ));
    }

    #[test]
    fn critical_battery_returns_after_configured_snooze() {
        let now = 1_800_000_000;
        let thresholds = ThresholdConfig::default();
        let mut dismissals = Dismissals::default();
        let snapshot = fixture_snapshot().with_battery(6, false).build();
        let selected = select_context(&snapshot, now, &thresholds, &dismissals)
            .expect("expected critical battery");

        dismissals.dismiss(&selected, now, thresholds.critical_snooze_seconds);

        assert_eq!(
            select_context(&snapshot, now + 299, &thresholds, &dismissals),
            None
        );
        assert!(matches!(
            select_context(&snapshot, now + 300, &thresholds, &dismissals),
            Some(ContextCard::Battery { percent: 6, .. })
        ));
    }

    #[test]
    fn completed_timer_returns_after_configured_snooze() {
        let now = 1_800_000_000;
        let thresholds = ThresholdConfig::default();
        let mut dismissals = Dismissals::default();
        let snapshot = fixture_snapshot()
            .with_timer("tea", 0, Some(now), true)
            .build();
        let selected = select_context(&snapshot, now, &thresholds, &dismissals)
            .expect("expected completed timer");

        dismissals.dismiss(&selected, now, thresholds.critical_snooze_seconds);

        assert_eq!(
            select_context(&snapshot, now + 299, &thresholds, &dismissals),
            None
        );
        assert!(matches!(
            select_context(&snapshot, now + 300, &thresholds, &dismissals),
            Some(ContextCard::Timer {
                ref id,
                completed: true,
                ..
            }) if id == "tea"
        ));
    }

    #[test]
    fn newer_timer_wins_when_tier_and_deadline_are_equal() {
        let now = 1_800_000_000;
        let deadline = now + 60;
        let mut snapshot = fixture_snapshot().build();
        snapshot.system.timers = vec![
            TimerState {
                id: "newer".to_string(),
                label: "newer".to_string(),
                remaining_seconds: 60,
                target_epoch: Some(deadline),
                completed: false,
                changed_at: now - 5,
            },
            TimerState {
                id: "older".to_string(),
                label: "older".to_string(),
                remaining_seconds: 60,
                target_epoch: Some(deadline),
                completed: false,
                changed_at: now - 10,
            },
        ];

        assert!(matches!(
            select_context(
                &snapshot,
                now,
                &ThresholdConfig::default(),
                &Dismissals::default()
            ),
            Some(ContextCard::Timer { ref id, .. }) if id == "newer"
        ));
    }

    #[test]
    fn newer_timer_beats_calendar_when_tier_and_deadline_are_equal() {
        let now = 1_800_000_000;
        let deadline = now + 60;
        let mut snapshot = fixture_snapshot().build();
        snapshot.system.calendar = Some(CalendarEvent {
            id: "review".to_string(),
            title: "Review".to_string(),
            location: None,
            start_epoch: deadline,
            end_epoch: Some(deadline + 60 * 60),
            changed_at: now - 10,
        });
        snapshot.system.timers = vec![TimerState {
            id: "tea".to_string(),
            label: "Tea".to_string(),
            remaining_seconds: 60,
            target_epoch: Some(deadline),
            completed: false,
            changed_at: now - 5,
        }];

        assert!(matches!(
            select_context(
                &snapshot,
                now,
                &ThresholdConfig::default(),
                &Dismissals::default()
            ),
            Some(ContextCard::Timer { ref id, .. }) if id == "tea"
        ));
    }

    #[test]
    fn newer_urgent_window_beats_battery_when_tier_and_deadline_are_equal() {
        let now = 1_800_000_000;
        let mut snapshot = fixture_snapshot()
            .with_urgent_window("window-42", "Build failed")
            .with_battery(6, false)
            .build();
        snapshot
            .outputs
            .get_mut("DP-5")
            .and_then(|output| output.focused_window.as_mut())
            .expect("expected focused window")
            .changed_at = now - 5;
        snapshot.system.power.changed_at = now - 10;

        assert!(matches!(
            select_context(
                &snapshot,
                now,
                &ThresholdConfig::default(),
                &Dismissals::default()
            ),
            Some(ContextCard::Urgent {
                ref window_id,
                ..
            }) if window_id.as_deref() == Some("window-42")
        ));
    }

    #[derive(Clone, Default)]
    struct SnapshotFixture {
        snapshot: BarSnapshot,
    }

    fn fixture_snapshot() -> SnapshotFixture {
        SnapshotFixture::default()
    }

    impl SnapshotFixture {
        fn with_build(mut self, label: &str, status: ActivityStatus) -> Self {
            let completed = status != ActivityStatus::Running;
            self.snapshot.activities = ActivityState {
                items: BTreeMap::from([(
                    label.to_string(),
                    CommandActivity {
                        id: label.to_string(),
                        label: label.to_string(),
                        cwd: PathBuf::from("/tmp/project"),
                        status,
                        started_at: 1_799_999_900,
                        finished_at: completed.then_some(1_800_000_000),
                        exit_code: completed.then_some(0),
                    },
                )]),
            };
            self
        }

        fn with_event(mut self, id: &str, start_epoch: i64) -> Self {
            self.snapshot.system.calendar = Some(CalendarEvent {
                id: id.to_string(),
                title: format!("{id} title"),
                location: Some("Room 1".to_string()),
                start_epoch,
                end_epoch: Some(start_epoch + 60 * 60),
                changed_at: 1_799_999_950,
            });
            self
        }

        fn with_battery(mut self, percent: u8, charging: bool) -> Self {
            self.snapshot.system.power = PowerState {
                battery_percent: Some(percent),
                charging,
                profile: PowerProfile::Balanced,
                changed_at: 1_799_999_950,
            };
            self
        }

        fn with_timer(
            mut self,
            id: &str,
            remaining_seconds: u64,
            target_epoch: Option<i64>,
            completed: bool,
        ) -> Self {
            self.snapshot.system.timers = vec![TimerState {
                id: id.to_string(),
                label: id.to_string(),
                remaining_seconds,
                target_epoch,
                completed,
                changed_at: 1_799_999_950,
            }];
            self
        }

        fn with_urgent_window(mut self, id: &str, title: &str) -> Self {
            self.snapshot.outputs.insert(
                "DP-5".to_string(),
                OutputState {
                    name: "DP-5".to_string(),
                    workspaces: Vec::new(),
                    windows: vec![WindowState {
                        id: id.to_string(),
                        app_id: Some("terminal".to_string()),
                        title: title.to_string(),
                        urgent: true,
                        workspace_id: None,
                        changed_at: 1_799_999_950,
                    }],
                    focused_window: Some(WindowState {
                        id: id.to_string(),
                        app_id: Some("terminal".to_string()),
                        title: title.to_string(),
                        urgent: true,
                        workspace_id: None,
                        changed_at: 1_799_999_950,
                    }),
                    urgent: false,
                    changed_at: 1_799_999_950,
                },
            );
            self
        }

        #[allow(dead_code)]
        fn with_media(mut self, title: &str) -> Self {
            self.snapshot.system.media = Some(MediaState {
                player: "player".to_string(),
                status: PlaybackStatus::Playing,
                title: Some(title.to_string()),
                artist: Some("Artist".to_string()),
                changed_at: 1_799_999_950,
            });
            self
        }

        fn build(self) -> BarSnapshot {
            self.snapshot
        }
    }
}
