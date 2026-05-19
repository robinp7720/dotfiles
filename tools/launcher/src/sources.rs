use crate::model::{
    Action, PasswordOperation, PowerOperation, QueryInput, ResultItem, SearchMode, SourceFilter,
    WindowFocusTarget, browser_target, score_text,
};
use crate::prediction::{PredictionStore, StoredPrediction};
use gtk4::gio;
use gtk4::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_APPS: usize = 8;
const MAX_WINDOWS: usize = 8;
const MAX_FILES: usize = 8;
const MAX_SSH: usize = 6;
const MAX_PASS: usize = 8;
const MAX_COMMANDS: usize = 8;
const MAX_POWER_ACTIONS: usize = 5;
const MAX_BOOKMARKS: usize = 8;
const MAX_RECENTS: usize = 8;
const MIN_FILE_QUERY_CHARS: usize = 2;

struct PowerAction {
    operation: PowerOperation,
    title: &'static str,
    subtitle: &'static str,
    icon_name: &'static str,
    search_terms: &'static [&'static str],
}

const POWER_ACTIONS: &[PowerAction] = &[
    PowerAction {
        operation: PowerOperation::Lock,
        title: "Lock",
        subtitle: "Blank the screen and keep the session running",
        icon_name: "system-lock-screen-symbolic",
        search_terms: &["lock", "screen lock", "secure", "power", "session"],
    },
    PowerAction {
        operation: PowerOperation::Suspend,
        title: "Suspend",
        subtitle: "Lock first, then suspend the machine",
        icon_name: "media-playback-pause-symbolic",
        search_terms: &["suspend", "sleep", "standby", "power", "session"],
    },
    PowerAction {
        operation: PowerOperation::Logout,
        title: "Logout",
        subtitle: "Close the current desktop session after confirmation",
        icon_name: "system-log-out-symbolic",
        search_terms: &[
            "logout",
            "log out",
            "sign out",
            "exit session",
            "power",
            "session",
        ],
    },
    PowerAction {
        operation: PowerOperation::Reboot,
        title: "Reboot",
        subtitle: "Restart the system after confirmation",
        icon_name: "system-reboot-symbolic",
        search_terms: &["reboot", "restart", "power", "session"],
    },
    PowerAction {
        operation: PowerOperation::Shutdown,
        title: "Shutdown",
        subtitle: "Power off the system after confirmation",
        icon_name: "system-shutdown-symbolic",
        search_terms: &[
            "shutdown",
            "shut down",
            "poweroff",
            "power off",
            "power",
            "session",
        ],
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileSearchBackend {
    LocalSearch,
    Tracker3,
}

impl FileSearchBackend {
    fn detect() -> Option<Self> {
        if command_exists("localsearch") {
            Some(Self::LocalSearch)
        } else if command_exists("tracker3") {
            Some(Self::Tracker3)
        } else {
            None
        }
    }

    fn run_search(self, query: &str, limit: usize) -> std::io::Result<std::process::Output> {
        match self {
            Self::LocalSearch => Command::new("localsearch")
                .args(["search", "-f", "--limit", &limit.to_string(), query])
                .output(),
            Self::Tracker3 => Command::new("tracker3")
                .args(["search", "--limit", &limit.to_string(), query])
                .output(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub desktop_id: String,
    pub name: String,
    pub description: String,
    pub executable: String,
    pub icon_name: String,
    pub search_blob: String,
}

#[derive(Clone, Debug)]
pub struct PassEntry {
    pub name: String,
    pub search_blob: String,
}

#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub title: String,
    pub app_name: String,
    pub workspace: String,
    pub search_blob: String,
    pub focus_order: i64,
    pub focus_target: WindowFocusTarget,
}

#[derive(Clone, Debug)]
pub struct BookmarkEntry {
    pub title: String,
    pub url: String,
    pub search_blob: String,
}

#[derive(Clone, Debug)]
pub struct RecentFileEntry {
    pub title: String,
    pub path: String,
    pub modified: i64,
    pub search_blob: String,
}

#[derive(Clone, Debug)]
pub struct Sources {
    apps: Vec<AppEntry>,
    ssh_hosts: Vec<String>,
    pass_entries: Vec<PassEntry>,
    commands: Vec<String>,
    bookmarks: Vec<BookmarkEntry>,
    recent_files: Vec<RecentFileEntry>,
    file_search_backend: Option<FileSearchBackend>,
    pass_available: bool,
    qalc_available: bool,
    predictions: Arc<Mutex<PredictionStore>>,
}

impl Sources {
    pub fn load() -> Self {
        Self {
            apps: load_applications(),
            ssh_hosts: load_ssh_hosts(),
            pass_entries: load_pass_entries(),
            commands: load_commands(),
            bookmarks: load_browser_bookmarks(),
            recent_files: load_recent_files(),
            file_search_backend: FileSearchBackend::detect(),
            pass_available: command_exists("pass"),
            qalc_available: command_exists("qalc"),
            predictions: Arc::new(Mutex::new(PredictionStore::load())),
        }
    }

    pub fn warm_external_sources(&self) {
        if let Some(backend) = self.file_search_backend {
            thread::spawn(move || {
                let _ = backend.run_search("a", 1);
            });
        }
    }

    pub fn search(&self, raw_query: &str, cli_mode: SearchMode) -> Vec<ResultItem> {
        let query = QueryInput::parse(raw_query, cli_mode);
        let mut results = Vec::new();
        let now = current_unix_time();

        if query.text.is_empty() {
            results.extend(self.default_results(&query, now));
            return results;
        }

        match query.source_filter {
            SourceFilter::Bookmarks => {
                results.extend(self.search_bookmarks(&query, now));
                return finish_search_results(results, &query);
            }
            SourceFilter::Recents => {
                results.extend(self.search_recent_files(&query, now));
                return finish_search_results(results, &query);
            }
            SourceFilter::All => {}
        }

        if query.mode.includes(SearchMode::Apps) {
            results.extend(self.search_apps(&query, now));
        }

        if query.mode.includes(SearchMode::Windows) {
            results.extend(self.search_windows(&query, now));
        }

        if query.mode.includes(SearchMode::Files) {
            results.extend(self.search_files(&query, now));
        }

        if query.mode.includes(SearchMode::Ssh) {
            results.extend(self.search_ssh(&query, now));
        }

        if query.mode.includes(SearchMode::Pass) {
            results.extend(self.search_pass(&query, now));
        }

        if matches!(query.mode, SearchMode::All | SearchMode::Commands) {
            results.extend(self.search_power(&query, now));
        }

        if query.mode == SearchMode::All {
            results.extend(self.search_bookmarks(&query, now));
            results.extend(self.search_recent_files(&query, now));
        }

        if query.mode == SearchMode::Commands {
            results.extend(self.search_commands(&query, now));
        } else if query.mode == SearchMode::All {
            if let Some(result) = self.search_all_mode_command(&query, now) {
                results.push(result);
            }
        }

        if query.mode.includes(SearchMode::Calc) {
            if let Some(result) = self.search_calc(&query, now) {
                results.push(result);
            }
        }

        if let Some(result) = self.search_url(&query, now) {
            results.push(result);
        }

        if query.mode == SearchMode::Web {
            results.push(self.search_web(&query, now));
        }

        finish_search_results(results, &query)
    }

    pub fn record_activation(&self, item: &ResultItem) {
        let Some(key) = item.prediction_key.clone() else {
            return;
        };
        if matches!(item.action, Action::None) {
            return;
        }

        let prediction = StoredPrediction {
            key,
            title: item.title.clone(),
            subtitle: item.subtitle.clone(),
            source: item.source.to_string(),
            icon_name: item.icon_name.clone(),
            action: item.action.clone(),
        };

        if let Ok(mut predictions) = self.predictions.lock() {
            let _ = predictions.record(prediction, current_unix_time());
        }
    }

    fn default_results(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut results = Vec::new();
        let mode = query.mode;

        match query.source_filter {
            SourceFilter::Bookmarks => {
                results.push(instruction_result(
                    "Bookmark search",
                    "Type a bookmark title or URL fragment",
                    "Bookmarks",
                    "user-bookmarks-symbolic",
                    65,
                ));
                return results;
            }
            SourceFilter::Recents => {
                results.push(instruction_result(
                    "Recent file search",
                    "Type a recently used file name or path fragment",
                    "Recent Files",
                    "document-open-recent-symbolic",
                    65,
                ));
                return results;
            }
            SourceFilter::All => {}
        }

        if mode == SearchMode::All {
            results.extend(self.top_prediction_results(now));
        }

        if mode.includes(SearchMode::Apps) {
            results.extend(self.apps.iter().take(8).map(|app| ResultItem {
                prediction_key: Some(app_prediction_key(&app.desktop_id)),
                title: app.name.clone(),
                subtitle: if app.description.is_empty() {
                    app.executable.clone()
                } else {
                    app.description.clone()
                },
                source: "Applications",
                icon_name: app.icon_name.clone(),
                score: 80,
                action: Action::LaunchApp {
                    desktop_id: app.desktop_id.clone(),
                },
            }));
        }

        if mode == SearchMode::Windows {
            let windows = load_windows();
            if windows.is_empty() {
                results.push(instruction_result(
                    "No active windows found",
                    "Hyprland or Niri did not report switchable windows",
                    "Windows",
                    "view-grid-symbolic",
                    65,
                ));
            } else {
                results.extend(
                    windows
                        .into_iter()
                        .take(MAX_WINDOWS)
                        .map(window_result_item),
                );
            }
        }

        if mode.includes(SearchMode::Ssh) {
            results.extend(self.ssh_hosts.iter().take(4).map(|host| ResultItem {
                prediction_key: Some(ssh_prediction_key(host)),
                title: host.clone(),
                subtitle: "Open an SSH session".to_string(),
                source: "SSH",
                icon_name: "network-server-symbolic".to_string(),
                score: 70,
                action: Action::Ssh { host: host.clone() },
            }));
        }

        if mode == SearchMode::Pass {
            if !self.pass_available {
                results.push(instruction_result(
                    "pass is not installed",
                    "Install pass to search password-store entries",
                    "Passwords",
                    "dialog-password-symbolic",
                    65,
                ));
            } else if self.pass_entries.is_empty() {
                results.push(instruction_result(
                    "Password store is empty",
                    "Add entries to ~/.password-store or set PASSWORD_STORE_DIR",
                    "Passwords",
                    "dialog-password-symbolic",
                    65,
                ));
            } else {
                results.push(instruction_result(
                    "Password mode",
                    "Type an entry name and press Enter to autotype its login",
                    "Passwords",
                    "dialog-password-symbolic",
                    65,
                ));
            }
        }

        if mode == SearchMode::Files {
            if self.file_search_backend.is_some() {
                results.push(instruction_result(
                    "File mode",
                    "Type a name or path fragment to search indexed files",
                    "Files",
                    "system-search-symbolic",
                    65,
                ));
            } else {
                results.push(instruction_result(
                    "Indexed file search unavailable",
                    "Install LocalSearch to enable indexed file search",
                    "Files",
                    "system-search-symbolic",
                    65,
                ));
            }
        }

        if mode == SearchMode::Commands {
            results.push(instruction_result(
                "Command mode",
                "Type a shell command and press Enter to run it",
                "Commands",
                "utilities-terminal-symbolic",
                65,
            ));
        }

        if mode == SearchMode::Web {
            results.push(instruction_result(
                "Web mode",
                "Type a query and press Enter to search the web",
                "Web",
                "web-browser-symbolic",
                65,
            ));
        }

        if mode == SearchMode::Calc {
            if self.qalc_available {
                results.push(instruction_result(
                    "Calculator mode",
                    "Type an expression like 2+2 and press Enter to copy the result",
                    "Calculator",
                    "accessories-calculator-symbolic",
                    65,
                ));
            } else {
                results.push(instruction_result(
                    "qalc is not installed",
                    "Install libqalculate to enable calculator results",
                    "Calculator",
                    "accessories-calculator-symbolic",
                    65,
                ));
            }
        }

        results
    }

    fn search_apps(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = self
            .apps
            .iter()
            .filter_map(|app| {
                let score = score_text(&app.search_blob, &query.text)?;
                let prediction_key = app_prediction_key(&app.desktop_id);
                Some(ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: app.name.clone(),
                    subtitle: if app.description.is_empty() {
                        app.executable.clone()
                    } else {
                        app.description.clone()
                    },
                    source: "Applications",
                    icon_name: app.icon_name.clone(),
                    score: score + 900 + self.prediction_boost(&prediction_key, now),
                    action: Action::LaunchApp {
                        desktop_id: app.desktop_id.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_APPS);
        items
    }

    fn search_windows(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = load_windows()
            .into_iter()
            .filter_map(|window| {
                let score = score_text(&window.search_blob, &query.text)?;
                let prediction_key = window_prediction_key(&window);
                let boosted_score = score + 860 + self.prediction_boost(&prediction_key, now);
                Some(window_result_item_with_score(window, boosted_score))
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_WINDOWS);
        items
    }

    fn search_files(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        if query.text.chars().count() < MIN_FILE_QUERY_CHARS {
            if query.mode == SearchMode::Files {
                return vec![instruction_result(
                    "Keep typing to search files",
                    "Type at least 2 characters before querying the file index",
                    "Files",
                    "system-search-symbolic",
                    520,
                )];
            }
            return Vec::new();
        }

        let Some(backend) = self.file_search_backend else {
            if query.mode == SearchMode::Files {
                return vec![ResultItem {
                    prediction_key: None,
                    title: "Indexed file search unavailable".to_string(),
                    subtitle: "Install LocalSearch to enable indexed file search".to_string(),
                    source: "Files",
                    icon_name: "system-search-symbolic".to_string(),
                    score: 500,
                    action: Action::None,
                }];
            }
            return Vec::new();
        };

        let Ok(output) = backend.run_search(&query.text, MAX_FILES) else {
            return Vec::new();
        };

        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(parse_file_search_line)
            .take(MAX_FILES)
            .map(|path| {
                let file_name = Path::new(&path)
                    .file_name()
                    .and_then(|part| part.to_str())
                    .unwrap_or(path.as_str())
                    .to_string();
                ResultItem {
                    prediction_key: Some(file_prediction_key(&path)),
                    title: file_name,
                    subtitle: path.clone(),
                    source: "Files",
                    icon_name: "folder-documents-symbolic".to_string(),
                    score: 760 + self.prediction_boost(&file_prediction_key(&path), now),
                    action: Action::OpenFile { path },
                }
            })
            .collect()
    }

    fn search_ssh(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = self
            .ssh_hosts
            .iter()
            .filter_map(|host| {
                let score = score_text(host, &query.text)?;
                let prediction_key = ssh_prediction_key(host);
                Some(ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: host.clone(),
                    subtitle: "Open an SSH session".to_string(),
                    source: "SSH",
                    icon_name: "network-server-symbolic".to_string(),
                    score: score + 720 + self.prediction_boost(&prediction_key, now),
                    action: Action::Ssh { host: host.clone() },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_SSH);
        items
    }

    fn search_pass(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        if !self.pass_available {
            if query.mode == SearchMode::Pass {
                return vec![ResultItem {
                    prediction_key: None,
                    title: "pass is not installed".to_string(),
                    subtitle: "Install pass to search password-store entries".to_string(),
                    source: "Passwords",
                    icon_name: "dialog-password-symbolic".to_string(),
                    score: 500,
                    action: Action::None,
                }];
            }
            return Vec::new();
        }

        let mut items = Vec::new();
        for entry in &self.pass_entries {
            let Some(score) = score_text(&entry.search_blob, &query.text) else {
                continue;
            };
            let prediction_key = pass_prediction_key(&entry.name);
            let boosted_score = score + 880 + self.prediction_boost(&prediction_key, now);
            items.extend(password_action_results(
                &entry.name,
                boosted_score,
                query.mode == SearchMode::Pass,
            ));
        }

        sort_results(&mut items, MAX_PASS);
        items
    }

    fn search_commands(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = Vec::new();
        let run_prediction_key = command_prediction_key(&query.text);
        items.push(ResultItem {
            prediction_key: Some(run_prediction_key.clone()),
            title: format!("Run “{}”", query.text),
            subtitle: "Execute in the background with sh -lc".to_string(),
            source: "Commands",
            icon_name: "utilities-terminal-symbolic".to_string(),
            score: 930 + self.prediction_boost(&run_prediction_key, now),
            action: Action::RunCommand {
                command: query.text.clone(),
            },
        });

        let mut suggestions = self
            .commands
            .iter()
            .filter_map(|command| {
                let score = score_text(command, &query.text)?;
                let prediction_key = command_prediction_key(command);
                Some(ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: command.clone(),
                    subtitle: "Executable from $PATH".to_string(),
                    source: "Commands",
                    icon_name: "utilities-terminal-symbolic".to_string(),
                    score: score + 700 + self.prediction_boost(&prediction_key, now),
                    action: Action::RunCommand {
                        command: command.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();
        sort_results(&mut suggestions, MAX_COMMANDS);
        items.extend(suggestions);

        items
    }

    fn search_power(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = POWER_ACTIONS
            .iter()
            .filter_map(|action| {
                power_action_score(action, &query.text).map(|score| (action, score))
            })
            .map(|(action, score)| {
                let prediction_key = power_prediction_key(action.operation);
                ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: action.title.to_string(),
                    subtitle: action.subtitle.to_string(),
                    source: "Power",
                    icon_name: action.icon_name.to_string(),
                    score: score + 950 + self.prediction_boost(&prediction_key, now),
                    action: Action::Power {
                        operation: action.operation,
                        confirmed: false,
                    },
                }
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_POWER_ACTIONS);
        items
    }

    fn search_bookmarks(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = self
            .bookmarks
            .iter()
            .filter_map(|bookmark| {
                let score = score_text(&bookmark.search_blob, &query.text)?;
                let prediction_key = bookmark_prediction_key(&bookmark.url);
                Some(ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: bookmark.title.clone(),
                    subtitle: bookmark.url.clone(),
                    source: "Bookmarks",
                    icon_name: "user-bookmarks-symbolic".to_string(),
                    score: score + 830 + self.prediction_boost(&prediction_key, now),
                    action: Action::OpenUrl {
                        url: bookmark.url.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_BOOKMARKS);
        items
    }

    fn search_recent_files(&self, query: &QueryInput, now: u64) -> Vec<ResultItem> {
        let mut items = self
            .recent_files
            .iter()
            .enumerate()
            .filter_map(|(index, recent)| {
                let score = score_text(&recent.search_blob, &query.text)?;
                let prediction_key = recent_prediction_key(&recent.path);
                let recency_score = (MAX_RECENTS.saturating_sub(index.min(MAX_RECENTS)) * 5) as i32;
                Some(ResultItem {
                    prediction_key: Some(prediction_key.clone()),
                    title: recent.title.clone(),
                    subtitle: recent.path.clone(),
                    source: "Recent Files",
                    icon_name: "document-open-recent-symbolic".to_string(),
                    score: score
                        + 790
                        + recency_score
                        + self.prediction_boost(&prediction_key, now),
                    action: Action::OpenFile {
                        path: recent.path.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_RECENTS);
        items
    }

    fn search_all_mode_command(&self, query: &QueryInput, now: u64) -> Option<ResultItem> {
        let mut words = query.text.split_whitespace();
        let program = words.next()?;
        words.next()?;

        if !self.commands.iter().any(|command| command == program) {
            return None;
        }

        let prediction_key = command_prediction_key(&query.text);
        Some(ResultItem {
            prediction_key: Some(prediction_key.clone()),
            title: format!("Run \"{}\"", query.text),
            subtitle: "Execute in the background with sh -lc".to_string(),
            source: "Commands",
            icon_name: "utilities-terminal-symbolic".to_string(),
            score: 930 + self.prediction_boost(&prediction_key, now),
            action: Action::RunCommand {
                command: query.text.clone(),
            },
        })
    }

    fn search_calc(&self, query: &QueryInput, now: u64) -> Option<ResultItem> {
        if !self.qalc_available {
            return None;
        }

        if !looks_like_math(&query.text) && query.mode == SearchMode::All {
            return None;
        }

        let output = Command::new("qalc")
            .args(["-t", "--terse", &query.text])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if result.is_empty() || result.eq_ignore_ascii_case("error") {
            return None;
        }

        let prediction_key = calc_prediction_key(&query.text);
        Some(ResultItem {
            prediction_key: Some(prediction_key.clone()),
            title: result.clone(),
            subtitle: format!("Result for {}", query.text),
            source: "Calculator",
            icon_name: "accessories-calculator-symbolic".to_string(),
            score: 1_100 + self.prediction_boost(&prediction_key, now),
            action: Action::CopyText { text: result },
        })
    }

    fn search_url(&self, query: &QueryInput, now: u64) -> Option<ResultItem> {
        if !matches!(query.mode, SearchMode::All | SearchMode::Web) {
            return None;
        }

        let url = browser_target(&query.text)?;
        let prediction_key = url_prediction_key(&url);
        Some(ResultItem {
            prediction_key: Some(prediction_key.clone()),
            title: format!("Open {url}"),
            subtitle: "Open URL in the default browser".to_string(),
            source: "Web",
            icon_name: "web-browser-symbolic".to_string(),
            score: 1_200 + self.prediction_boost(&prediction_key, now),
            action: Action::OpenUrl { url },
        })
    }

    fn search_web(&self, query: &QueryInput, now: u64) -> ResultItem {
        let prediction_key = web_prediction_key(&query.text);
        ResultItem {
            prediction_key: Some(prediction_key.clone()),
            title: format!("Search the web for “{}”", query.text),
            subtitle: "Open the default browser".to_string(),
            source: "Web",
            icon_name: "web-browser-symbolic".to_string(),
            score: 120 + self.prediction_boost(&prediction_key, now),
            action: Action::WebSearch {
                query: query.text.clone(),
            },
        }
    }

    fn prediction_boost(&self, key: &str, now: u64) -> i32 {
        self.predictions
            .lock()
            .map(|predictions| predictions.boost_for_key(key, now))
            .unwrap_or_default()
    }

    fn top_prediction_results(&self, now: u64) -> Vec<ResultItem> {
        self.predictions
            .lock()
            .map(|predictions| predictions.top_results(8, now))
            .unwrap_or_default()
    }
}

fn load_applications() -> Vec<AppEntry> {
    let mut apps = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show())
        .filter_map(|app| {
            let desktop_id = app.id()?.to_string();
            let name = app.display_name().to_string();
            let executable = app.executable().to_string_lossy().to_string();
            let description = app
                .description()
                .map(|text| text.to_string())
                .unwrap_or_default();
            let icon_name = app
                .icon()
                .and_then(|icon| icon.dynamic_cast::<gio::ThemedIcon>().ok())
                .and_then(|icon| icon.names().first().map(|name| name.to_string()))
                .unwrap_or_else(|| "application-x-executable-symbolic".to_string());

            Some(AppEntry {
                search_blob: format!("{name} {description} {executable}").to_ascii_lowercase(),
                desktop_id,
                name,
                description,
                executable,
                icon_name,
            })
        })
        .collect::<Vec<_>>();

    apps.sort_by(|left, right| left.name.cmp(&right.name));
    apps
}

fn load_ssh_hosts() -> Vec<String> {
    let mut hosts = BTreeSet::new();
    if let Some(home) = dirs::home_dir() {
        parse_ssh_config(&home.join(".ssh/config"), &mut hosts);
        parse_known_hosts(&home.join(".ssh/known_hosts"), &mut hosts);
        parse_known_hosts(&home.join(".ssh/known_hosts.old"), &mut hosts);
    }
    hosts.into_iter().collect()
}

fn load_browser_bookmarks() -> Vec<BookmarkEntry> {
    let mut by_url = BTreeMap::new();

    for entry in load_firefox_bookmarks()
        .into_iter()
        .chain(load_chromium_bookmarks())
    {
        by_url.entry(entry.url.clone()).or_insert(entry);
    }

    let mut bookmarks = by_url.into_values().collect::<Vec<_>>();
    bookmarks.sort_by(|left, right| {
        left.title
            .to_ascii_lowercase()
            .cmp(&right.title.to_ascii_lowercase())
            .then_with(|| left.url.cmp(&right.url))
    });
    bookmarks
}

fn load_firefox_bookmarks() -> Vec<BookmarkEntry> {
    if !command_exists("sqlite3") {
        return Vec::new();
    }

    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let profiles_dir = home.join(".mozilla/firefox");
    let Ok(profiles) = fs::read_dir(profiles_dir) else {
        return Vec::new();
    };

    let query = "select replace(coalesce(b.title,''), char(9), ' '), p.url \
        from moz_bookmarks b join moz_places p on p.id = b.fk \
        where b.type = 1 and p.url not like 'place:%'";

    profiles
        .flatten()
        .map(|profile| profile.path().join("places.sqlite"))
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let database = format!("file:{}?immutable=1", path.to_string_lossy());
            let output = Command::new("sqlite3")
                .args(["-separator", "\t", &database, query])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            Some(parse_firefox_bookmark_rows(&String::from_utf8_lossy(
                &output.stdout,
            )))
        })
        .flatten()
        .collect()
}

fn load_chromium_bookmarks() -> Vec<BookmarkEntry> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let roots = [
        home.join(".config/google-chrome"),
        home.join(".config/chromium"),
        home.join(".config/BraveSoftware/Brave-Browser"),
        home.join(".config/vivaldi"),
    ];

    roots
        .into_iter()
        .flat_map(chromium_bookmark_files)
        .filter_map(|path| fs::read_to_string(path).ok())
        .flat_map(|contents| parse_chromium_bookmarks_json(&contents))
        .collect()
}

fn chromium_bookmark_files(root: PathBuf) -> Vec<PathBuf> {
    let Ok(profiles) = fs::read_dir(root) else {
        return Vec::new();
    };

    profiles
        .flatten()
        .map(|profile| profile.path().join("Bookmarks"))
        .filter(|path| path.is_file())
        .collect()
}

fn load_windows() -> Vec<WindowEntry> {
    if command_exists("hyprctl") {
        if let Ok(output) = Command::new("hyprctl").args(["clients", "-j"]).output() {
            if output.status.success() {
                if let Ok(windows) =
                    parse_hypr_windows_json(&String::from_utf8_lossy(&output.stdout))
                {
                    if !windows.is_empty() {
                        return windows;
                    }
                }
            }
        }
    }

    if command_exists("niri") {
        if let Ok(output) = Command::new("niri")
            .args(["msg", "windows", "--json"])
            .output()
        {
            if output.status.success() {
                if let Ok(windows) =
                    parse_niri_windows_json(&String::from_utf8_lossy(&output.stdout))
                {
                    return windows;
                }
            }
        }
    }

    Vec::new()
}

pub fn focus_window(target: &WindowFocusTarget) -> std::io::Result<std::process::ExitStatus> {
    let (program, args) = window_focus_command(target);
    Command::new(program).args(args).status()
}

pub fn focused_window_target() -> Option<WindowFocusTarget> {
    if command_exists("hyprctl") {
        if let Ok(output) = Command::new("hyprctl")
            .args(["activewindow", "-j"])
            .output()
        {
            if output.status.success() {
                let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?;
                if let Some(address) = string_field(&parsed, "address") {
                    if !address.is_empty() && address != "0x0" {
                        let xwayland = bool_field(&parsed, "xwayland").unwrap_or(false);
                        return Some(WindowFocusTarget::Hyprland { address, xwayland });
                    }
                }
            }
        }
    }

    if command_exists("niri") {
        if let Ok(output) = Command::new("niri")
            .args(["msg", "focused-window", "--json"])
            .output()
        {
            if output.status.success() {
                let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?;
                if let Some(id) = parsed.get("id").and_then(serde_json::Value::as_u64) {
                    return Some(WindowFocusTarget::Niri { id });
                }
            }
        }
    }

    if command_exists("xdotool") {
        if let Ok(output) = Command::new("xdotool").arg("getactivewindow").output() {
            if output.status.success() {
                let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !window_id.is_empty() {
                    return Some(WindowFocusTarget::X11 { window_id });
                }
            }
        }
    }

    None
}

pub fn window_focus_command(target: &WindowFocusTarget) -> (&'static str, Vec<String>) {
    match target {
        WindowFocusTarget::Hyprland { address, .. } => (
            "hyprctl",
            vec![
                "dispatch".to_string(),
                "focuswindow".to_string(),
                format!("address:{address}"),
            ],
        ),
        WindowFocusTarget::Niri { id } => (
            "niri",
            vec![
                "msg".to_string(),
                "action".to_string(),
                "focus-window".to_string(),
                "--id".to_string(),
                id.to_string(),
            ],
        ),
        WindowFocusTarget::X11 { window_id } => (
            "xdotool",
            vec![
                "windowactivate".to_string(),
                "--sync".to_string(),
                window_id.clone(),
            ],
        ),
    }
}

pub fn parse_hypr_windows_json(raw: &str) -> serde_json::Result<Vec<WindowEntry>> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let windows = value.as_array().into_iter().flatten();
    let mut entries = windows
        .filter(|window| bool_field(window, "mapped").unwrap_or(true))
        .filter(|window| !bool_field(window, "hidden").unwrap_or(false))
        .filter_map(|window| {
            let address = string_field(window, "address")?;
            let title = string_field(window, "title")
                .filter(|title| !title.trim().is_empty())
                .or_else(|| string_field(window, "initialTitle"))
                .unwrap_or_else(|| "Untitled window".to_string());
            let app_name = string_field(window, "class")
                .filter(|class| !class.trim().is_empty())
                .or_else(|| string_field(window, "initialClass"))
                .unwrap_or_else(|| "Unknown app".to_string());
            let workspace = window
                .get("workspace")
                .and_then(|workspace| string_field(workspace, "name"))
                .or_else(|| {
                    window
                        .get("workspace")
                        .and_then(|workspace| number_field(workspace, "id"))
                        .map(|id| id.to_string())
                })
                .unwrap_or_else(|| "unknown".to_string());
            let focus_order = number_field(window, "focusHistoryID").unwrap_or(i64::MAX);
            let search_blob =
                format!("{title} {app_name} workspace {workspace}").to_ascii_lowercase();

            Some(WindowEntry {
                title,
                app_name,
                workspace,
                search_blob,
                focus_order,
                focus_target: WindowFocusTarget::Hyprland {
                    address,
                    xwayland: bool_field(window, "xwayland").unwrap_or(false),
                },
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left.focus_order
            .cmp(&right.focus_order)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(entries)
}

pub fn parse_niri_windows_json(raw: &str) -> serde_json::Result<Vec<WindowEntry>> {
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let windows = value.as_array().into_iter().flatten();
    let mut entries = windows
        .enumerate()
        .filter_map(|(index, window)| {
            let id = unsigned_field(window, "id")?;
            let title = string_field(window, "title")
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "Untitled window".to_string());
            let app_name = string_field(window, "app_id")
                .filter(|app_id| !app_id.trim().is_empty())
                .or_else(|| string_field(window, "app_id_or_class"))
                .unwrap_or_else(|| "Unknown app".to_string());
            let workspace = string_field(window, "workspace_name")
                .or_else(|| {
                    number_field(window, "workspace_id")
                        .map(|workspace_id| workspace_id.to_string())
                })
                .unwrap_or_else(|| "unknown".to_string());
            let focus_order = number_field(window, "focus_order")
                .or_else(|| number_field(window, "last_focus_time"))
                .unwrap_or(index as i64);
            let search_blob =
                format!("{title} {app_name} workspace {workspace}").to_ascii_lowercase();

            Some(WindowEntry {
                title,
                app_name,
                workspace,
                search_blob,
                focus_order,
                focus_target: WindowFocusTarget::Niri { id },
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        left.focus_order
            .cmp(&right.focus_order)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(entries)
}

fn window_result_item(window: WindowEntry) -> ResultItem {
    let score = 760 - window.focus_order.min(200) as i32;
    window_result_item_with_score(window, score)
}

fn window_result_item_with_score(window: WindowEntry, score: i32) -> ResultItem {
    let prediction_key = window_prediction_key(&window);
    ResultItem {
        prediction_key: Some(prediction_key),
        title: window.title,
        subtitle: format!("{} on workspace {}", window.app_name, window.workspace),
        source: "Windows",
        icon_name: "view-grid-symbolic".to_string(),
        score,
        action: Action::FocusWindow {
            target: window.focus_target,
        },
    }
}

fn app_prediction_key(desktop_id: &str) -> String {
    format!("app:{desktop_id}")
}

fn window_prediction_key(window: &WindowEntry) -> String {
    format!(
        "window:{}:{}:{}",
        window.app_name, window.title, window.workspace
    )
}

fn file_prediction_key(path: &str) -> String {
    format!("file:{path}")
}

fn ssh_prediction_key(host: &str) -> String {
    format!("ssh:{host}")
}

fn pass_prediction_key(entry: &str) -> String {
    format!("pass:{entry}")
}

fn bookmark_prediction_key(url: &str) -> String {
    format!("bookmark:{url}")
}

fn recent_prediction_key(path: &str) -> String {
    format!("recent:{path}")
}

fn password_action_results(entry: &str, score: i32, include_secondary: bool) -> Vec<ResultItem> {
    let mut rows = vec![password_action_result(
        entry,
        "Autotype login",
        "Type username, Tab, and password without submitting",
        PasswordOperation::AutotypeLogin,
        score + 80,
        Some(pass_prediction_key(entry)),
    )];

    if include_secondary {
        rows.extend([
            password_action_result(
                entry,
                "Copy password",
                "Copy password and clear it after the password-store timeout",
                PasswordOperation::CopyPassword,
                score + 50,
                None,
            ),
            password_action_result(
                entry,
                "Copy username",
                "Copy username metadata or the entry basename",
                PasswordOperation::CopyUsername,
                score + 45,
                None,
            ),
            password_action_result(
                entry,
                "Type password",
                "Type only the password into the focused window",
                PasswordOperation::TypePassword,
                score + 40,
                None,
            ),
            password_action_result(
                entry,
                "Type username",
                "Type only the username into the focused window",
                PasswordOperation::TypeUsername,
                score + 35,
                None,
            ),
            password_action_result(
                entry,
                "Inspect actions",
                "Decrypt this entry to show URL, OTP, and custom actions",
                PasswordOperation::Inspect,
                score + 30,
                None,
            ),
        ]);
    }

    rows
}

fn password_action_result(
    entry: &str,
    title: &str,
    subtitle: &str,
    operation: PasswordOperation,
    score: i32,
    prediction_key: Option<String>,
) -> ResultItem {
    ResultItem {
        prediction_key,
        title: format!("{title}: {entry}"),
        subtitle: subtitle.to_string(),
        source: "Passwords",
        icon_name: "dialog-password-symbolic".to_string(),
        score,
        action: Action::Password {
            entry: entry.to_string(),
            operation,
        },
    }
}

fn command_prediction_key(command: &str) -> String {
    format!("cmd:{command}")
}

fn power_prediction_key(operation: PowerOperation) -> String {
    format!("power:{}", power_operation_id(operation))
}

fn power_action_score(action: &PowerAction, query: &str) -> Option<i32> {
    action
        .search_terms
        .iter()
        .filter_map(|term| score_text(term, query))
        .max()
}

fn power_operation_id(operation: PowerOperation) -> &'static str {
    match operation {
        PowerOperation::Lock => "lock",
        PowerOperation::Suspend => "suspend",
        PowerOperation::Logout => "logout",
        PowerOperation::Reboot => "reboot",
        PowerOperation::Shutdown => "shutdown",
    }
}

fn url_prediction_key(url: &str) -> String {
    format!("url:{url}")
}

fn web_prediction_key(query: &str) -> String {
    format!("web:{query}")
}

fn calc_prediction_key(expression: &str) -> String {
    format!("calc:{expression}")
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

fn number_field(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(serde_json::Value::as_i64)
}

fn unsigned_field(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn bool_field(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(serde_json::Value::as_bool)
}

fn parse_ssh_config(path: &Path, hosts: &mut BTreeSet<String>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        if !matches!(parts.next(), Some(keyword) if keyword.eq_ignore_ascii_case("host")) {
            continue;
        }

        for alias in parts {
            if alias.contains('*') || alias.contains('?') || alias.starts_with('!') {
                continue;
            }
            hosts.insert(alias.to_string());
        }
    }
}

fn parse_known_hosts(path: &Path, hosts: &mut BTreeSet<String>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };

    for line in contents.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let Some(field) = line.split_whitespace().next() else {
            continue;
        };

        if field.starts_with('|') {
            continue;
        }

        for host in field.split(',') {
            let cleaned = host.trim_matches(|ch| ch == '[' || ch == ']');
            let cleaned = cleaned.split(':').next().unwrap_or(cleaned).trim();
            if !cleaned.is_empty() {
                hosts.insert(cleaned.to_string());
            }
        }
    }
}

fn load_commands() -> Vec<String> {
    let mut commands = BTreeSet::new();
    let mut seen_dirs = HashSet::new();

    for dir in env::var_os("PATH")
        .unwrap_or_default()
        .to_string_lossy()
        .split(':')
        .map(PathBuf::from)
    {
        if !seen_dirs.insert(dir.clone()) {
            continue;
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }

            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                commands.insert(name.to_string());
            }
        }
    }

    commands.into_iter().collect()
}

fn load_pass_entries() -> Vec<PassEntry> {
    let Some(store_dir) = password_store_dir() else {
        return Vec::new();
    };

    let mut stack = vec![store_dir.clone()];
    let mut entries = Vec::new();

    while let Some(dir) = stack.pop() {
        let Ok(children) = fs::read_dir(&dir) else {
            continue;
        };

        for child in children.flatten() {
            let path = child.path();
            let Ok(file_type) = child.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let Some(name) = pass_entry_name(&store_dir, &path) else {
                continue;
            };

            entries.push(PassEntry {
                search_blob: name.to_ascii_lowercase(),
                name,
            });
        }
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

fn load_recent_files() -> Vec<RecentFileEntry> {
    let Some(data_dir) = dirs::data_dir() else {
        return Vec::new();
    };
    let path = data_dir.join("recently-used.xbel");
    fs::read_to_string(path)
        .map(|contents| parse_recent_files_xbel(&contents))
        .unwrap_or_default()
}

fn parse_chromium_bookmarks_json(raw: &str) -> Vec<BookmarkEntry> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    if let Some(roots) = value.get("roots").and_then(serde_json::Value::as_object) {
        for root in roots.values() {
            collect_chromium_bookmarks(root, &mut entries);
        }
    }
    entries
}

fn collect_chromium_bookmarks(value: &serde_json::Value, entries: &mut Vec<BookmarkEntry>) {
    if value.get("type").and_then(serde_json::Value::as_str) == Some("url") {
        if let Some(url) = string_field(value, "url") {
            let title = string_field(value, "name").unwrap_or_else(|| url.clone());
            if let Some(entry) = bookmark_entry(title, url) {
                entries.push(entry);
            }
        }
    }

    if let Some(children) = value.get("children").and_then(serde_json::Value::as_array) {
        for child in children {
            collect_chromium_bookmarks(child, entries);
        }
    }
}

fn parse_firefox_bookmark_rows(raw: &str) -> Vec<BookmarkEntry> {
    raw.lines()
        .filter_map(|line| {
            let (title, url) = line.split_once('\t')?;
            bookmark_entry(title.trim().to_string(), url.trim().to_string())
        })
        .collect()
}

fn bookmark_entry(title: String, url: String) -> Option<BookmarkEntry> {
    let url = url.trim();
    if url.is_empty() || url.starts_with("place:") {
        return None;
    }

    let title = if title.trim().is_empty() {
        url.to_string()
    } else {
        title.trim().to_string()
    };

    Some(BookmarkEntry {
        search_blob: format!("{title} {url}").to_ascii_lowercase(),
        title,
        url: url.to_string(),
    })
}

fn parse_recent_files_xbel(raw: &str) -> Vec<RecentFileEntry> {
    use xml::reader::{EventReader, XmlEvent};

    let parser = EventReader::from_str(raw);
    let mut entries = Vec::new();
    let mut href = None::<String>;
    let mut modified = 0;
    let mut title = String::new();
    let mut in_title = false;

    for event in parser {
        match event {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "bookmark" => {
                href = attributes
                    .iter()
                    .find(|attribute| attribute.name.local_name == "href")
                    .map(|attribute| attribute.value.clone());
                modified = attributes
                    .iter()
                    .find(|attribute| attribute.name.local_name == "modified")
                    .or_else(|| {
                        attributes
                            .iter()
                            .find(|attribute| attribute.name.local_name == "visited")
                    })
                    .and_then(|attribute| parse_xbel_timestamp(&attribute.value))
                    .unwrap_or_default();
                title.clear();
            }
            Ok(XmlEvent::StartElement { name, .. }) if name.local_name == "title" => {
                in_title = true;
                title.clear();
            }
            Ok(XmlEvent::Characters(text)) if in_title => title.push_str(&text),
            Ok(XmlEvent::EndElement { name }) if name.local_name == "title" => {
                in_title = false;
            }
            Ok(XmlEvent::EndElement { name }) if name.local_name == "bookmark" => {
                if let Some(href) = href.take() {
                    if let Some(entry) = recent_file_entry(&href, &title, modified) {
                        entries.push(entry);
                    }
                }
                title.clear();
                modified = 0;
                in_title = false;
            }
            _ => {}
        }
    }

    entries.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.title.cmp(&right.title))
    });
    let mut seen_paths = BTreeSet::new();
    entries.retain(|entry| seen_paths.insert(entry.path.clone()));
    entries
}

fn recent_file_entry(href: &str, title: &str, modified: i64) -> Option<RecentFileEntry> {
    let path = file_uri_to_path(href)?;
    let title = if title.trim().is_empty() {
        Path::new(&path)
            .file_name()
            .and_then(|part| part.to_str())
            .unwrap_or(path.as_str())
            .to_string()
    } else {
        title.trim().to_string()
    };

    Some(RecentFileEntry {
        search_blob: format!("{title} {path}").to_ascii_lowercase(),
        title,
        path,
        modified,
    })
}

fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest
        .strip_prefix("localhost/")
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| rest.to_string());
    if !path.starts_with('/') {
        return None;
    }
    urlencoding::decode(&path)
        .ok()
        .map(|path| path.into_owned())
        .filter(|path| !path.is_empty())
}

fn parse_xbel_timestamp(raw: &str) -> Option<i64> {
    let digits = raw
        .chars()
        .filter(char::is_ascii_digit)
        .take(14)
        .collect::<String>();
    if digits.len() < 8 {
        return None;
    }
    digits.parse().ok()
}

fn password_store_dir() -> Option<PathBuf> {
    env::var_os("PASSWORD_STORE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".password-store")))
        .filter(|path| path.is_dir())
}

fn pass_entry_name(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let relative = relative.to_string_lossy();
    let name = relative.strip_suffix(".gpg")?;
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn parse_file_search_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = trimmed
        .split_once(' ')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(trimmed);

    if candidate.starts_with("file://") {
        let file = gio::File::for_uri(candidate);
        if let Some(path) = file.path() {
            return Some(path.to_string_lossy().to_string());
        }

        let decoded = urlencoding::decode(candidate.strip_prefix("file://")?).ok()?;
        return Some(decoded.into_owned());
    }

    Some(candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        AppEntry, BookmarkEntry, FileSearchBackend, RecentFileEntry, Sources, no_results_item,
        parse_chromium_bookmarks_json, parse_file_search_line, parse_firefox_bookmark_rows,
        parse_hypr_windows_json, parse_niri_windows_json, parse_recent_files_xbel, pass_entry_name,
        window_focus_command,
    };
    use crate::model::{
        Action, PasswordOperation, PowerOperation, QueryInput, SearchMode, SourceFilter,
        WindowFocusTarget,
    };
    use crate::prediction::{PredictionStore, StoredPrediction};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    fn empty_prediction_store() -> Arc<Mutex<PredictionStore>> {
        Arc::new(Mutex::new(PredictionStore::disabled()))
    }

    fn prediction_store_with(
        prediction: StoredPrediction,
        now: u64,
    ) -> Arc<Mutex<PredictionStore>> {
        let mut store = PredictionStore::disabled();
        store.record(prediction, now).expect("record prediction");
        Arc::new(Mutex::new(store))
    }

    fn empty_sources() -> Sources {
        Sources {
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            bookmarks: Vec::new(),
            recent_files: Vec::new(),
            file_search_backend: None,
            pass_available: false,
            qalc_available: false,
            predictions: empty_prediction_store(),
        }
    }

    #[test]
    fn indexed_paths_are_uri_decoded() {
        let line = "file:///tmp/with%20space%23hash.txt";
        assert_eq!(
            parse_file_search_line(line).as_deref(),
            Some("/tmp/with space#hash.txt")
        );
    }

    #[test]
    fn parses_hyprland_windows_for_switching() {
        let windows = parse_hypr_windows_json(
            r#"[
              {
                "address": "0xabc",
                "class": "kitty",
                "title": "editor",
                "workspace": {"name": "2"},
                "mapped": true,
                "hidden": false,
                "xwayland": true,
                "focusHistoryID": 3
              },
              {
                "address": "0xdef",
                "class": "launcher",
                "title": "hidden",
                "workspace": {"name": "special"},
                "mapped": false,
                "hidden": true,
                "focusHistoryID": 9
              }
            ]"#,
        )
        .expect("parse hypr window json");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, "editor");
        assert_eq!(windows[0].app_name, "kitty");
        assert_eq!(windows[0].workspace, "2");
        assert_eq!(
            windows[0].focus_target,
            WindowFocusTarget::Hyprland {
                address: "0xabc".to_string(),
                xwayland: true
            }
        );
    }

    #[test]
    fn builds_native_focus_command_for_hyprland_window() {
        let (program, args) = window_focus_command(&WindowFocusTarget::Hyprland {
            address: "0xabc".to_string(),
            xwayland: false,
        });

        assert_eq!(program, "hyprctl");
        assert_eq!(args, vec!["dispatch", "focuswindow", "address:0xabc"]);
    }

    #[test]
    fn parses_niri_windows_for_switching() {
        let windows = parse_niri_windows_json(
            r#"[
              {
                "id": 42,
                "app_id": "firefox",
                "title": "Docs",
                "workspace_id": 7
              }
            ]"#,
        )
        .expect("parse niri window json");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, "Docs");
        assert_eq!(windows[0].app_name, "firefox");
        assert_eq!(windows[0].workspace, "7");
        assert_eq!(windows[0].focus_target, WindowFocusTarget::Niri { id: 42 });
    }

    #[test]
    fn builds_native_focus_command_for_niri_window() {
        let (program, args) = window_focus_command(&WindowFocusTarget::Niri { id: 42 });

        assert_eq!(program, "niri");
        assert_eq!(args, vec!["msg", "action", "focus-window", "--id", "42"]);
    }

    #[test]
    fn builds_focus_command_for_x11_window() {
        let (program, args) = window_focus_command(&WindowFocusTarget::X11 {
            window_id: "12345".to_string(),
        });

        assert_eq!(program, "xdotool");
        assert_eq!(args, vec!["windowactivate", "--sync", "12345"]);
    }

    #[test]
    fn search_returns_status_item_when_no_matches_exist() {
        let sources = empty_sources();

        let results = sources.search("unlikely-query", SearchMode::Apps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "Status");
        assert!(matches!(results[0].action, Action::None));
    }

    #[test]
    fn bare_urls_surface_as_browser_results_in_all_mode() {
        let sources = empty_sources();

        let results = sources.search("example.com/docs", SearchMode::All);
        assert!(matches!(
            results.first().map(|item| &item.action),
            Some(Action::OpenUrl { url }) if url == "https://example.com/docs"
        ));
    }

    #[test]
    fn no_results_item_uses_mode_specific_guidance() {
        let item = no_results_item(&QueryInput {
            mode: SearchMode::Files,
            source_filter: SourceFilter::All,
            text: "report".to_string(),
        });

        assert_eq!(item.title, "No matches for \"report\"");
        assert!(item.subtitle.contains("file indexer"));
        assert!(matches!(item.action, Action::None));
    }

    #[test]
    fn file_mode_requires_a_minimum_query_length_before_shelling_out() {
        let sources = Sources {
            file_search_backend: Some(FileSearchBackend::LocalSearch),
            ..empty_sources()
        };

        let results = sources.search("/ a", SearchMode::All);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "Files");
        assert_eq!(results[0].title, "Keep typing to search files");
        assert!(matches!(results[0].action, Action::None));
    }

    #[test]
    fn all_mode_password_matches_default_to_autotype_login() {
        let sources = Sources {
            pass_entries: vec![super::PassEntry {
                name: "github/work".to_string(),
                search_blob: "github/work".to_string(),
            }],
            pass_available: true,
            ..empty_sources()
        };

        let results = sources.search("pass: github", SearchMode::All);
        assert!(matches!(
            results.first().map(|item| &item.action),
            Some(Action::Password {
                entry,
                operation: PasswordOperation::AutotypeLogin,
            }) if entry == "github/work"
        ));
    }

    #[test]
    fn pass_mode_surfaces_action_rows_for_matching_entries() {
        let sources = Sources {
            pass_entries: vec![super::PassEntry {
                name: "github/work".to_string(),
                search_blob: "github/work".to_string(),
            }],
            pass_available: true,
            ..empty_sources()
        };

        let results = sources.search("pass: github", SearchMode::All);
        let operations = results
            .iter()
            .filter_map(|item| match &item.action {
                Action::Password { operation, .. } => Some(operation),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(operations.contains(&&PasswordOperation::AutotypeLogin));
        assert!(operations.contains(&&PasswordOperation::CopyPassword));
        assert!(operations.contains(&&PasswordOperation::CopyUsername));
        assert!(operations.contains(&&PasswordOperation::TypePassword));
        assert!(operations.contains(&&PasswordOperation::TypeUsername));
        assert!(operations.contains(&&PasswordOperation::Inspect));
    }

    #[test]
    fn learned_matches_are_boosted_in_search_results() {
        let sources = Sources {
            apps: vec![
                AppEntry {
                    desktop_id: "alpha.desktop".to_string(),
                    name: "Alpha Browser".to_string(),
                    description: "Web browser".to_string(),
                    executable: "alpha".to_string(),
                    icon_name: "alpha".to_string(),
                    search_blob: "alpha browser web browser".to_string(),
                },
                AppEntry {
                    desktop_id: "beta.desktop".to_string(),
                    name: "Beta Browser".to_string(),
                    description: "Web browser".to_string(),
                    executable: "beta".to_string(),
                    icon_name: "beta".to_string(),
                    search_blob: "beta browser web browser".to_string(),
                },
            ],
            predictions: prediction_store_with(
                StoredPrediction {
                    key: "app:beta.desktop".to_string(),
                    title: "Beta Browser".to_string(),
                    subtitle: "Web browser".to_string(),
                    source: "Applications".to_string(),
                    icon_name: "beta".to_string(),
                    action: Action::LaunchApp {
                        desktop_id: "beta.desktop".to_string(),
                    },
                },
                super::current_unix_time().saturating_sub(60),
            ),
            ..empty_sources()
        };

        let results = sources.search("browser", SearchMode::Apps);

        assert_eq!(results[0].title, "Beta Browser");
    }

    #[test]
    fn empty_all_mode_starts_with_learned_predictions() {
        let sources = Sources {
            apps: vec![AppEntry {
                desktop_id: "alpha.desktop".to_string(),
                name: "Alpha".to_string(),
                description: "First app".to_string(),
                executable: "alpha".to_string(),
                icon_name: "alpha".to_string(),
                search_blob: "alpha first app".to_string(),
            }],
            predictions: prediction_store_with(
                StoredPrediction {
                    key: "cmd:git status".to_string(),
                    title: "Run \"git status\"".to_string(),
                    subtitle: "Execute in the background with sh -lc".to_string(),
                    source: "Commands".to_string(),
                    icon_name: "utilities-terminal-symbolic".to_string(),
                    action: Action::RunCommand {
                        command: "git status".to_string(),
                    },
                },
                super::current_unix_time().saturating_sub(60),
            ),
            ..empty_sources()
        };

        let results = sources.search("", SearchMode::All);

        assert_eq!(results[0].source, "Commands");
        assert_eq!(results[0].title, "Run \"git status\"");
    }

    #[test]
    fn all_mode_surfaces_command_runner_when_input_starts_with_known_command() {
        let sources = Sources {
            commands: vec!["systemctl".to_string()],
            ..empty_sources()
        };

        let results = sources.search("systemctl suspend", SearchMode::All);

        assert_eq!(results[0].title, "Run \"systemctl suspend\"");
        assert!(matches!(
            &results[0].action,
            Action::RunCommand { command } if command == "systemctl suspend"
        ));
    }

    #[test]
    fn all_mode_surfaces_curated_power_actions() {
        let sources = empty_sources();

        let results = sources.search("reboot", SearchMode::All);

        assert_eq!(results[0].source, "Power");
        assert_eq!(results[0].title, "Reboot");
        assert!(matches!(
            &results[0].action,
            Action::Power {
                operation: PowerOperation::Reboot,
                confirmed: false,
            }
        ));
    }

    #[test]
    fn power_actions_match_common_synonyms() {
        let sources = empty_sources();

        let results = sources.search("sleep", SearchMode::All);

        assert_eq!(results[0].source, "Power");
        assert_eq!(results[0].title, "Suspend");
        assert!(matches!(
            &results[0].action,
            Action::Power {
                operation: PowerOperation::Suspend,
                confirmed: false,
            }
        ));
    }

    #[test]
    fn parses_chromium_bookmark_json_urls() {
        let bookmarks = parse_chromium_bookmarks_json(
            r#"{
              "roots": {
                "bookmark_bar": {
                  "type": "folder",
                  "children": [
                    {"type": "url", "name": "Rust", "url": "https://www.rust-lang.org/"},
                    {"type": "folder", "children": [
                      {"type": "url", "name": "", "url": "https://example.com/docs"}
                    ]}
                  ]
                }
              }
            }"#,
        );

        assert_eq!(bookmarks.len(), 2);
        assert_eq!(bookmarks[0].title, "Rust");
        assert_eq!(bookmarks[0].url, "https://www.rust-lang.org/");
        assert_eq!(bookmarks[1].title, "https://example.com/docs");
    }

    #[test]
    fn parses_firefox_sqlite_rows_as_bookmarks() {
        let bookmarks =
            parse_firefox_bookmark_rows("Rust Docs\thttps://doc.rust-lang.org/\n\tplace:sort=8\n");

        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].title, "Rust Docs");
        assert_eq!(bookmarks[0].url, "https://doc.rust-lang.org/");
    }

    #[test]
    fn parses_recent_files_xbel_and_skips_non_file_uris() {
        let recents = parse_recent_files_xbel(
            r#"<?xml version="1.0"?>
            <xbel>
              <bookmark href="file:///home/robin/Documents/Project%20Plan.pdf" modified="2026-05-05T10:11:12Z">
                <title>Project Plan</title>
              </bookmark>
              <bookmark href="https://example.com" modified="2026-05-06T10:11:12Z">
                <title>Remote</title>
              </bookmark>
              <bookmark href="file:///home/robin/Downloads/raw.txt" modified="2026-05-04T10:11:12Z"/>
            </xbel>"#,
        );

        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0].title, "Project Plan");
        assert_eq!(recents[0].path, "/home/robin/Documents/Project Plan.pdf");
        assert_eq!(recents[1].title, "raw.txt");
    }

    #[test]
    fn all_mode_searches_bookmarks_and_recent_files() {
        let sources = Sources {
            bookmarks: vec![BookmarkEntry {
                title: "Rust Documentation".to_string(),
                url: "https://doc.rust-lang.org/".to_string(),
                search_blob: "rust documentation https://doc.rust-lang.org/".to_string(),
            }],
            recent_files: vec![RecentFileEntry {
                title: "Project Plan".to_string(),
                path: "/home/robin/Documents/Project Plan.pdf".to_string(),
                modified: 20260505101112,
                search_blob: "project plan /home/robin/documents/project plan.pdf".to_string(),
            }],
            ..empty_sources()
        };

        let bookmark_results = sources.search("rust doc", SearchMode::All);
        assert!(matches!(
            bookmark_results.first().map(|item| &item.action),
            Some(Action::OpenUrl { url }) if url == "https://doc.rust-lang.org/"
        ));
        assert_eq!(
            bookmark_results[0].prediction_key.as_deref(),
            Some("bookmark:https://doc.rust-lang.org/")
        );

        let recent_results = sources.search("project plan", SearchMode::All);
        assert!(matches!(
            recent_results.first().map(|item| &item.action),
            Some(Action::OpenFile { path }) if path == "/home/robin/Documents/Project Plan.pdf"
        ));
        assert_eq!(
            recent_results[0].prediction_key.as_deref(),
            Some("recent:/home/robin/Documents/Project Plan.pdf")
        );
    }

    #[test]
    fn explicit_local_prefixes_search_only_the_selected_source() {
        let sources = Sources {
            bookmarks: vec![BookmarkEntry {
                title: "Project Board".to_string(),
                url: "https://example.com/project".to_string(),
                search_blob: "project board https://example.com/project".to_string(),
            }],
            recent_files: vec![RecentFileEntry {
                title: "Project Notes".to_string(),
                path: "/home/robin/project.txt".to_string(),
                modified: 20260505101112,
                search_blob: "project notes /home/robin/project.txt".to_string(),
            }],
            ..empty_sources()
        };

        let bookmark_results = sources.search("bookmark: project", SearchMode::All);
        assert_eq!(bookmark_results.len(), 1);
        assert_eq!(bookmark_results[0].source, "Bookmarks");

        let recent_results = sources.search("recent: project", SearchMode::All);
        assert_eq!(recent_results.len(), 1);
        assert_eq!(recent_results[0].source, "Recent Files");
    }

    #[test]
    fn empty_explicit_local_prefixes_show_instruction_rows() {
        let sources = empty_sources();

        let bookmark_results = sources.search("bookmark:", SearchMode::All);
        assert_eq!(bookmark_results[0].title, "Bookmark search");

        let recent_results = sources.search("recent:", SearchMode::All);
        assert_eq!(recent_results[0].title, "Recent file search");
    }

    #[test]
    fn pass_entry_names_are_derived_from_store_paths() {
        let root = Path::new("/tmp/store");
        let path = Path::new("/tmp/store/personal/github.gpg");
        assert_eq!(
            pass_entry_name(root, path).as_deref(),
            Some("personal/github")
        );
    }
}

fn command_exists(binary: &str) -> bool {
    env::var_os("PATH")
        .unwrap_or_default()
        .to_string_lossy()
        .split(':')
        .map(Path::new)
        .any(|dir| dir.join(binary).exists())
}

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn looks_like_math(query: &str) -> bool {
    query
        .chars()
        .any(|ch| ch.is_ascii_digit() || "+-*/^=()".contains(ch))
        || query.contains(" to ")
}

fn no_results_item(query: &QueryInput) -> ResultItem {
    let subtitle = match query.source_filter {
        SourceFilter::Bookmarks => "Try a different bookmark title or URL fragment.".to_string(),
        SourceFilter::Recents => "Try a different recently used file name.".to_string(),
        SourceFilter::All => match query.mode {
            SearchMode::All => "Try a broader term or switch to a dedicated mode.".to_string(),
            SearchMode::Apps => "Try a different app name or executable.".to_string(),
            SearchMode::Windows => "Try a window title, app id, or workspace name.".to_string(),
            SearchMode::Files => {
                "Try a different file name or ensure the file indexer has indexed it.".to_string()
            }
            SearchMode::Ssh => {
                "Check ~/.ssh/config and known_hosts for the expected host.".to_string()
            }
            SearchMode::Pass => "Try a different password-store entry name.".to_string(),
            SearchMode::Commands => {
                "Try a different executable name or a full shell command.".to_string()
            }
            SearchMode::Web => "Press Enter to open a browser search result instead.".to_string(),
            SearchMode::Calc => "Try a valid libqalculate expression such as 42/7.".to_string(),
        },
    };

    ResultItem {
        prediction_key: None,
        title: format!("No matches for \"{}\"", query.text),
        subtitle,
        source: "Status",
        icon_name: "system-search-symbolic".to_string(),
        score: 0,
        action: Action::None,
    }
}

fn sort_results(results: &mut Vec<ResultItem>, limit: usize) {
    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    results.truncate(limit);
}

fn finish_search_results(mut results: Vec<ResultItem>, query: &QueryInput) -> Vec<ResultItem> {
    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    results.truncate(24);
    if results.is_empty() {
        results.push(no_results_item(query));
    }
    results
}

fn instruction_result(
    title: &str,
    subtitle: &str,
    source: &'static str,
    icon_name: &str,
    score: i32,
) -> ResultItem {
    ResultItem {
        prediction_key: None,
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        source,
        icon_name: icon_name.to_string(),
        score,
        action: Action::None,
    }
}
