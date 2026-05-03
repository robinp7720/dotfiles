use crate::model::{Action, ResultItem};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredPrediction {
    pub key: String,
    pub title: String,
    pub subtitle: String,
    pub source: String,
    pub icon_name: String,
    pub action: Action,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PredictionEntry {
    #[serde(flatten)]
    pub prediction: StoredPrediction,
    pub count: u32,
    pub last_used: u64,
}

#[derive(Debug)]
pub struct PredictionStore {
    path: Option<PathBuf>,
    entries: BTreeMap<String, PredictionEntry>,
}

impl PredictionStore {
    pub fn load() -> Self {
        prediction_state_path()
            .and_then(|path| Self::load_from_path(path).ok())
            .unwrap_or_else(Self::disabled)
    }

    pub fn load_from_path(path: PathBuf) -> io::Result<Self> {
        let entries = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error),
        };

        Ok(Self {
            path: Some(path),
            entries,
        })
    }

    pub fn disabled() -> Self {
        Self {
            path: None,
            entries: BTreeMap::new(),
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn entry(&self, key: &str) -> Option<&PredictionEntry> {
        self.entries.get(key)
    }

    pub fn record(&mut self, prediction: StoredPrediction, now: u64) -> io::Result<()> {
        let entry = self
            .entries
            .entry(prediction.key.clone())
            .or_insert_with(|| PredictionEntry {
                prediction: prediction.clone(),
                count: 0,
                last_used: now,
            });
        entry.prediction = prediction;
        entry.count = entry.count.saturating_add(1);
        entry.last_used = now;
        self.prune();
        self.save()
    }

    pub fn boost_for_key(&self, key: &str, now: u64) -> i32 {
        let Some(entry) = self.entries.get(key) else {
            return 0;
        };

        let frequency = (entry.count.min(10) as i32) * 25;
        let age = now.saturating_sub(entry.last_used);
        let recency = if age <= 60 * 60 {
            250
        } else if age <= 24 * 60 * 60 {
            180
        } else if age <= 7 * 24 * 60 * 60 {
            100
        } else if age <= 30 * 24 * 60 * 60 {
            40
        } else {
            0
        };

        frequency + recency
    }

    pub fn top_results(&self, limit: usize, now: u64) -> Vec<ResultItem> {
        let mut entries = self.entries.values().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            self.boost_for_key(&right.prediction.key, now)
                .cmp(&self.boost_for_key(&left.prediction.key, now))
                .then_with(|| right.last_used.cmp(&left.last_used))
                .then_with(|| right.count.cmp(&left.count))
                .then_with(|| left.prediction.title.cmp(&right.prediction.title))
        });

        entries
            .into_iter()
            .take(limit)
            .map(|entry| ResultItem {
                title: entry.prediction.title.clone(),
                subtitle: entry.prediction.subtitle.clone(),
                source: source_label(&entry.prediction.source),
                icon_name: entry.prediction.icon_name.clone(),
                score: 2_000 + self.boost_for_key(&entry.prediction.key, now),
                action: entry.prediction.action.clone(),
                prediction_key: Some(entry.prediction.key.clone()),
            })
            .collect()
    }

    fn prune(&mut self) {
        if self.entries.len() <= MAX_HISTORY_ENTRIES {
            return;
        }

        let mut keys = self
            .entries
            .values()
            .map(|entry| (entry.last_used, entry.count, entry.prediction.key.clone()))
            .collect::<Vec<_>>();
        keys.sort();

        let remove_count = self.entries.len() - MAX_HISTORY_ENTRIES;
        for (_, _, key) in keys.into_iter().take(remove_count) {
            self.entries.remove(&key);
        }
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(&self.entries)?;
        fs::write(path, contents)
    }
}

fn source_label(source: &str) -> &'static str {
    match source {
        "Applications" => "Applications",
        "Windows" => "Windows",
        "Files" => "Files",
        "SSH" => "SSH",
        "Passwords" => "Passwords",
        "Commands" => "Commands",
        "Web" => "Web",
        "Calculator" => "Calculator",
        _ => "Predictions",
    }
}

fn prediction_state_path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
        .map(|state| state.join("dot-launcher/predictions.json"))
}

#[cfg(test)]
mod tests {
    use super::{PredictionStore, StoredPrediction};
    use crate::model::Action;
    use std::fs;

    fn temp_history_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dot-launcher-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn missing_history_loads_as_empty() {
        let path = temp_history_path("missing");
        let store = PredictionStore::load_from_path(path).expect("load missing history");

        assert_eq!(store.len(), 0);
    }

    #[test]
    fn recording_a_prediction_increments_count_and_updates_last_used() {
        let path = temp_history_path("record");
        let mut store = PredictionStore::load_from_path(path.clone()).expect("load history");
        let prediction = StoredPrediction {
            key: "app:firefox.desktop".to_string(),
            title: "Firefox".to_string(),
            subtitle: "Browser".to_string(),
            source: "Applications".to_string(),
            icon_name: "firefox".to_string(),
            action: Action::LaunchApp {
                desktop_id: "firefox.desktop".to_string(),
            },
        };

        store
            .record(prediction.clone(), 1_000)
            .expect("record prediction");
        store.record(prediction, 2_000).expect("record prediction");

        let reloaded = PredictionStore::load_from_path(path.clone()).expect("reload history");
        let entry = reloaded.entry("app:firefox.desktop").expect("stored entry");
        assert_eq!(entry.count, 2);
        assert_eq!(entry.last_used, 2_000);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_history_loads_as_empty() {
        let path = temp_history_path("invalid");
        fs::write(&path, "{not-json").expect("write invalid history");

        let store = PredictionStore::load_from_path(path.clone()).expect("load invalid history");

        assert_eq!(store.len(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn prediction_boost_combines_frequency_and_recency() {
        let mut store = PredictionStore::disabled();
        let prediction = StoredPrediction {
            key: "ssh:prometheus".to_string(),
            title: "prometheus".to_string(),
            subtitle: "Open an SSH session".to_string(),
            source: "SSH".to_string(),
            icon_name: "network-server-symbolic".to_string(),
            action: Action::Ssh {
                host: "prometheus".to_string(),
            },
        };

        store.record(prediction, 10_000).expect("record prediction");

        assert!(store.boost_for_key("ssh:prometheus", 10_100) > 0);
        assert_eq!(store.boost_for_key("ssh:unknown", 10_100), 0);
        assert!(
            store.boost_for_key("ssh:prometheus", 10_100)
                > store.boost_for_key("ssh:prometheus", 10_000 + 40 * 24 * 60 * 60)
        );
    }

    #[test]
    fn history_is_pruned_to_recent_entries() {
        let mut store = PredictionStore::disabled();

        for index in 0..510 {
            store
                .record(
                    StoredPrediction {
                        key: format!("cmd:command-{index}"),
                        title: format!("command-{index}"),
                        subtitle: "Execute in the background with sh -lc".to_string(),
                        source: "Commands".to_string(),
                        icon_name: "utilities-terminal-symbolic".to_string(),
                        action: Action::RunCommand {
                            command: format!("command-{index}"),
                        },
                    },
                    index,
                )
                .expect("record prediction");
        }

        assert_eq!(store.len(), 500);
        assert!(store.entry("cmd:command-0").is_none());
        assert!(store.entry("cmd:command-509").is_some());
    }

    #[test]
    fn top_predictions_are_returned_as_result_items() {
        let mut store = PredictionStore::disabled();
        store
            .record(
                StoredPrediction {
                    key: "app:firefox.desktop".to_string(),
                    title: "Firefox".to_string(),
                    subtitle: "Browser".to_string(),
                    source: "Applications".to_string(),
                    icon_name: "firefox".to_string(),
                    action: Action::LaunchApp {
                        desktop_id: "firefox.desktop".to_string(),
                    },
                },
                1_000,
            )
            .expect("record prediction");

        let results = store.top_results(10, 1_100);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Firefox");
        assert_eq!(results[0].source, "Applications");
        assert!(matches!(results[0].action, Action::LaunchApp { .. }));
    }
}
