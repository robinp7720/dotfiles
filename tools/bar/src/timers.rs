use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{ControlRequest, TimerState};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerRecord {
    pub id: String,
    pub label: String,
    pub remaining_seconds: u64,
    pub target_epoch: Option<i64>,
    pub completed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTimers {
    #[serde(default)]
    timers: Vec<TimerRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimerStore {
    state_path: PathBuf,
    records: Vec<TimerRecord>,
}

impl TimerStore {
    pub fn load(now_epoch: i64) -> Result<Self> {
        let state_path = timer_state_path()?;
        Self::load_at(state_path, now_epoch)
    }

    fn load_at(state_path: PathBuf, now_epoch: i64) -> Result<Self> {
        let records = match fs::read_to_string(&state_path) {
            Ok(contents) => {
                serde_json::from_str::<PersistedTimers>(&contents)
                    .with_context(|| format!("failed to parse {}", state_path.display()))?
                    .timers
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read {}", state_path.display()));
            }
        };

        let mut store = Self {
            state_path,
            records,
        };
        if store.refresh_completed(now_epoch)? {
            store.persist()?;
        }
        Ok(store)
    }

    pub fn apply(&mut self, request: &ControlRequest, now_epoch: i64) -> Result<Vec<TimerState>> {
        if self.refresh_completed(now_epoch)? {
            self.persist()?;
        }

        match request {
            ControlRequest::TimerStart {
                label,
                duration_seconds,
            } => {
                if *duration_seconds == 0 {
                    bail!("timer duration must be greater than zero");
                }
                self.records.push(TimerRecord {
                    id: next_timer_id(),
                    label: label.clone(),
                    remaining_seconds: *duration_seconds,
                    target_epoch: Some(
                        now_epoch
                            .checked_add(i64::try_from(*duration_seconds).unwrap_or(i64::MAX))
                            .unwrap_or(i64::MAX),
                    ),
                    completed: false,
                });
                self.persist()?;
            }
            ControlRequest::TimerPause { id } => {
                let record = self
                    .find_timer_mut(id)
                    .ok_or_else(|| anyhow!("unknown timer id: {id}"))?;
                if !record.completed
                    && let Some(target_epoch) = record.target_epoch
                {
                    record.remaining_seconds = remaining_seconds(target_epoch, now_epoch);
                    record.target_epoch = None;
                    self.persist()?;
                }
            }
            ControlRequest::TimerResume { id } => {
                let record = self
                    .find_timer_mut(id)
                    .ok_or_else(|| anyhow!("unknown timer id: {id}"))?;
                if !record.completed && record.target_epoch.is_none() {
                    record.target_epoch = Some(
                        now_epoch
                            .checked_add(
                                i64::try_from(record.remaining_seconds).unwrap_or(i64::MAX),
                            )
                            .unwrap_or(i64::MAX),
                    );
                    self.persist()?;
                }
            }
            ControlRequest::TimerCancel { id } => {
                let original_len = self.records.len();
                self.records.retain(|record| record.id != *id);
                if self.records.len() == original_len {
                    bail!("unknown timer id: {id}");
                }
                self.persist()?;
            }
            ControlRequest::TimerList => {}
            ControlRequest::ActivityStart { .. }
            | ControlRequest::ActivityFinish { .. }
            | ControlRequest::ContextGet { .. }
            | ControlRequest::ContextExecute { .. }
            | ControlRequest::ControlCenterOpen { .. } => {
                bail!("timer store cannot apply non-timer control requests");
            }
        }

        self.snapshot(now_epoch)
    }

    pub fn snapshot(&mut self, now_epoch: i64) -> Result<Vec<TimerState>> {
        if self.refresh_completed(now_epoch)? {
            self.persist()?;
        }

        Ok(self
            .records
            .iter()
            .map(|record| TimerState {
                id: record.id.clone(),
                label: record.label.clone(),
                remaining_seconds: timer_remaining_seconds(record, now_epoch),
                target_epoch: record.target_epoch,
                completed: record.completed,
                changed_at: 0,
            })
            .collect())
    }

    fn refresh_completed(&mut self, now_epoch: i64) -> Result<bool> {
        let mut changed = false;
        for record in &mut self.records {
            if record.completed {
                continue;
            }
            let Some(target_epoch) = record.target_epoch else {
                continue;
            };
            if target_epoch <= now_epoch {
                record.remaining_seconds = 0;
                record.completed = true;
                changed = true;
            }
        }
        Ok(changed)
    }

    fn find_timer_mut(&mut self, id: &str) -> Option<&mut TimerRecord> {
        self.records.iter_mut().find(|record| record.id == id)
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let payload = PersistedTimers {
            timers: self.records.clone(),
        };
        let temp_path = self.temp_path();
        let bytes = serde_json::to_vec(&payload).context("serialize timer store")?;
        fs::write(&temp_path, bytes)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        fs::rename(&temp_path, &self.state_path)
            .with_context(|| format!("failed to atomically replace {}", self.state_path.display()))
    }

    fn temp_path(&self) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        self.state_path.with_extension(format!("json.tmp-{unique}"))
    }
}

fn timer_state_path() -> Result<PathBuf> {
    let state_root = dirs::state_dir()
        .ok_or_else(|| anyhow!("failed to resolve XDG state directory for cockpit-bar"))?;
    Ok(state_root.join("cockpit-bar").join("timers.json"))
}

fn next_timer_id() -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    format!("timer-{unique}")
}

fn timer_remaining_seconds(record: &TimerRecord, now_epoch: i64) -> u64 {
    if record.completed {
        0
    } else if let Some(target_epoch) = record.target_epoch {
        remaining_seconds(target_epoch, now_epoch)
    } else {
        record.remaining_seconds
    }
}

fn remaining_seconds(target_epoch: i64, now_epoch: i64) -> u64 {
    u64::try_from(target_epoch.saturating_sub(now_epoch)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::ipc::ControlRequest;

    use super::TimerStore;

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn timer_lifecycle_survives_restart_and_pause_resume_math() {
        let state_home = TempDir::new("timers-restart");
        let _guard = EnvVarGuard::set("XDG_STATE_HOME", state_home.path());
        let start_epoch = 1_800_000_000;

        let mut store = TimerStore::load(start_epoch).expect("load empty store");
        let started = store
            .apply(
                &ControlRequest::TimerStart {
                    label: "Focus".to_string(),
                    duration_seconds: 25 * 60,
                },
                start_epoch,
            )
            .expect("start timer");
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].label, "Focus");
        assert_eq!(started[0].remaining_seconds, 25 * 60);
        assert_eq!(started[0].target_epoch, Some(start_epoch + 25 * 60));
        assert!(!started[0].completed);
        let timer_id = started[0].id.clone();
        drop(store);

        let mut reloaded = TimerStore::load(start_epoch + 5 * 60).expect("reload running timer");
        let running = reloaded
            .snapshot(start_epoch + 5 * 60)
            .expect("snapshot running");
        assert_eq!(running[0].id, timer_id);
        assert_eq!(running[0].remaining_seconds, 20 * 60);
        assert_eq!(running[0].target_epoch, Some(start_epoch + 25 * 60));
        assert!(!running[0].completed);

        let paused = reloaded
            .apply(
                &ControlRequest::TimerPause {
                    id: timer_id.clone(),
                },
                start_epoch + 10 * 60,
            )
            .expect("pause timer");
        assert_eq!(paused[0].remaining_seconds, 15 * 60);
        assert_eq!(paused[0].target_epoch, None);
        assert!(!paused[0].completed);
        drop(reloaded);

        let mut paused_store = TimerStore::load(start_epoch + 15 * 60).expect("reload paused");
        let paused_snapshot = paused_store
            .snapshot(start_epoch + 15 * 60)
            .expect("snapshot paused");
        assert_eq!(paused_snapshot[0].remaining_seconds, 15 * 60);
        assert_eq!(paused_snapshot[0].target_epoch, None);
        assert!(!paused_snapshot[0].completed);

        let resumed = paused_store
            .apply(
                &ControlRequest::TimerResume {
                    id: timer_id.clone(),
                },
                start_epoch + 15 * 60,
            )
            .expect("resume timer");
        assert_eq!(resumed[0].remaining_seconds, 15 * 60);
        assert_eq!(resumed[0].target_epoch, Some(start_epoch + 30 * 60));
        assert!(!resumed[0].completed);
        drop(paused_store);

        let mut completed_store =
            TimerStore::load(start_epoch + 30 * 60 + 1).expect("reload completed");
        let completed = completed_store
            .snapshot(start_epoch + 30 * 60 + 1)
            .expect("snapshot completed");
        assert_eq!(completed[0].remaining_seconds, 0);
        assert_eq!(completed[0].target_epoch, Some(start_epoch + 30 * 60));
        assert!(completed[0].completed);

        let persisted =
            fs::read_to_string(state_home.path().join("cockpit-bar").join("timers.json"))
                .expect("read persisted timers");
        assert!(persisted.contains("\"completed\":true"));
    }

    #[test]
    fn timer_list_returns_current_snapshot() {
        let state_home = TempDir::new("timers-list");
        let _guard = EnvVarGuard::set("XDG_STATE_HOME", state_home.path());
        let now = 1_800_000_000;

        let mut store = TimerStore::load(now).expect("load store");
        store
            .apply(
                &ControlRequest::TimerStart {
                    label: "Tea".to_string(),
                    duration_seconds: 180,
                },
                now,
            )
            .expect("start timer");

        let listed = store
            .apply(&ControlRequest::TimerList, now + 60)
            .expect("list timers");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "Tea");
        assert_eq!(listed[0].remaining_seconds, 120);
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("lock env");
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }
}
