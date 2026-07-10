use std::collections::{BTreeMap, BTreeSet};

use tracing::warn;

use crate::{ActivityState, ActivityStatus, ActivityUpdate, CommandActivity, ThresholdConfig};

const MAX_COMPLETED_ACTIVITIES: usize = 5;

#[derive(Clone, Debug)]
pub struct ActivityTracker {
    items: BTreeMap<String, CommandActivity>,
    unknown_finish_ids: BTreeSet<String>,
    completed_retention_seconds: u64,
    max_completed: usize,
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new(ThresholdConfig::default().work_completed_seconds)
    }
}

impl ActivityTracker {
    pub fn new(completed_retention_seconds: u64) -> Self {
        Self {
            items: BTreeMap::new(),
            unknown_finish_ids: BTreeSet::new(),
            completed_retention_seconds,
            max_completed: MAX_COMPLETED_ACTIVITIES,
        }
    }

    pub fn apply(&mut self, update: ActivityUpdate, now_epoch: i64) -> bool {
        let mut dirty = self.prune(now_epoch);

        dirty |= match update {
            ActivityUpdate::Started(activity) => self.start(activity),
            ActivityUpdate::Finished {
                id,
                finished_at,
                exit_code,
            } => self.finish(&id, finished_at, exit_code),
            ActivityUpdate::Snapshot(activities) => self.replace(activities),
            ActivityUpdate::Removed { id } => self.items.remove(&id).is_some(),
        };

        dirty |= self.prune(now_epoch);
        dirty |= self.trim_completed();
        dirty
    }

    pub fn prune(&mut self, now_epoch: i64) -> bool {
        let before_len = self.items.len();
        let retention_seconds = self.completed_retention_seconds as i64;
        self.items.retain(|_, activity| {
            let Some(finished_at) = activity.finished_at else {
                return true;
            };
            finished_at.saturating_add(retention_seconds) > now_epoch
        });
        self.items.len() != before_len
    }

    pub fn snapshot(&self) -> ActivityState {
        ActivityState {
            items: self.items.clone(),
        }
    }

    fn start(&mut self, activity: CommandActivity) -> bool {
        if self.items.contains_key(&activity.id) {
            false
        } else {
            self.items.insert(activity.id.clone(), activity);
            true
        }
    }

    fn finish(&mut self, id: &str, finished_at: i64, exit_code: i32) -> bool {
        let Some(activity) = self.items.get_mut(id) else {
            if self.unknown_finish_ids.insert(id.to_string()) {
                warn!("unknown activity finish id: {id}");
            }
            return false;
        };

        if activity.finished_at.is_some() {
            return false;
        }

        activity.status = if exit_code == 0 {
            ActivityStatus::Succeeded
        } else {
            ActivityStatus::Failed
        };
        activity.finished_at = Some(finished_at);
        activity.exit_code = Some(exit_code);
        true
    }

    fn replace(&mut self, activities: Vec<CommandActivity>) -> bool {
        let next_items = activities
            .into_iter()
            .map(|activity| (activity.id.clone(), activity))
            .collect::<BTreeMap<_, _>>();

        if self.items == next_items {
            false
        } else {
            self.items = next_items;
            true
        }
    }

    fn trim_completed(&mut self) -> bool {
        let completed_ids = self
            .items
            .iter()
            .filter_map(|(id, activity)| {
                activity
                    .finished_at
                    .map(|finished_at| (finished_at, id.clone()))
            })
            .collect::<Vec<_>>();

        let overflow = completed_ids.len().saturating_sub(self.max_completed);
        if overflow == 0 {
            return false;
        }

        let mut completed_ids = completed_ids;
        completed_ids.sort();
        for (_, id) in completed_ids.into_iter().take(overflow) {
            self.items.remove(&id);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::writer::MakeWriter;

    use super::ActivityTracker;
    use crate::{ActivityState, ActivityStatus, ActivityUpdate, CommandActivity};

    #[test]
    fn activity_tracker_ignores_duplicate_starts_and_only_finishes_matching_ids() {
        let mut tracker = ActivityTracker::new(30);

        let first_activity = started("shell-1", "Cargo test", "/tmp/project", 1_800_000_000);
        assert!(tracker.apply(
            ActivityUpdate::Started(first_activity.clone()),
            1_800_000_000,
        ));
        assert!(!tracker.apply(ActivityUpdate::Started(first_activity), 1_800_000_001,));

        assert!(!tracker.apply(
            ActivityUpdate::Finished {
                id: "missing".to_string(),
                finished_at: 1_800_000_002,
                exit_code: 0,
            },
            1_800_000_002,
        ));

        assert!(tracker.apply(
            ActivityUpdate::Finished {
                id: "shell-1".to_string(),
                finished_at: 1_800_000_003,
                exit_code: 0,
            },
            1_800_000_003,
        ));

        assert_eq!(
            tracker.snapshot(),
            ActivityState {
                items: [(
                    "shell-1".to_string(),
                    CommandActivity {
                        status: ActivityStatus::Succeeded,
                        finished_at: Some(1_800_000_003),
                        exit_code: Some(0),
                        ..started("shell-1", "Cargo test", "/tmp/project", 1_800_000_000)
                    },
                )]
                .into_iter()
                .collect(),
            }
        );
    }

    #[test]
    fn activity_tracker_limits_completed_cards_and_expires_old_ones() {
        let mut tracker = ActivityTracker::new(30);

        for index in 0..6 {
            let id = format!("shell-{index}");
            let started_at = 1_800_000_000 + index as i64;
            assert!(tracker.apply(
                ActivityUpdate::Started(started(&id, "Cargo test", "/tmp/project", started_at)),
                started_at,
            ));
            assert!(tracker.apply(
                ActivityUpdate::Finished {
                    id,
                    finished_at: started_at + 5,
                    exit_code: 0,
                },
                started_at + 5,
            ));
        }

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.items.len(), 5);
        assert!(!snapshot.items.contains_key("shell-0"));

        assert!(tracker.apply(
            ActivityUpdate::Started(started(
                "shell-running",
                "Pytest",
                "/tmp/project",
                1_800_000_100,
            )),
            1_800_000_100,
        ));

        let snapshot = tracker.snapshot();
        assert_eq!(snapshot.items.len(), 1);
        assert!(snapshot.items.contains_key("shell-running"));
        assert!(!tracker.prune(1_800_000_136));
    }

    #[test]
    fn activity_tracker_logs_unknown_finish_only_once() {
        let log_buffer = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .with_writer(SharedWriter(log_buffer.clone()))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let mut tracker = ActivityTracker::new(30);

            assert!(!tracker.apply(
                ActivityUpdate::Finished {
                    id: "missing".to_string(),
                    finished_at: 1_800_000_000,
                    exit_code: 1,
                },
                1_800_000_000,
            ));
            assert!(!tracker.apply(
                ActivityUpdate::Finished {
                    id: "missing".to_string(),
                    finished_at: 1_800_000_001,
                    exit_code: 1,
                },
                1_800_000_001,
            ));
        });

        let logs = String::from_utf8(log_buffer.lock().unwrap().clone()).unwrap();
        assert_eq!(
            logs.matches("unknown activity finish id: missing").count(),
            1
        );
    }

    fn started(id: &str, label: &str, cwd: &str, started_at: i64) -> CommandActivity {
        CommandActivity {
            id: id.to_string(),
            label: label.to_string(),
            cwd: PathBuf::from(cwd),
            status: ActivityStatus::Running,
            started_at,
            finished_at: None,
            exit_code: None,
        }
    }

    #[derive(Clone)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for SharedWriter {
        type Writer = SharedGuard;

        fn make_writer(&'a self) -> Self::Writer {
            SharedGuard(self.0.clone())
        }
    }

    struct SharedGuard(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SharedGuard {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
