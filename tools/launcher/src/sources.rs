use crate::model::{
    Action, PasswordOperation, QueryInput, ResultItem, SearchMode, WindowFocusTarget,
    browser_target, score_text,
};
use crate::prediction::{PredictionStore, StoredPrediction};
use gtk4::gio;
use gtk4::prelude::*;
use std::collections::{BTreeSet, HashSet};
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
const MIN_FILE_QUERY_CHARS: usize = 2;

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
pub struct Sources {
    apps: Vec<AppEntry>,
    ssh_hosts: Vec<String>,
    pass_entries: Vec<PassEntry>,
    commands: Vec<String>,
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
            results.extend(self.default_results(query.mode, now));
            return results;
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

        if query.mode == SearchMode::Commands {
            results.extend(self.search_commands(&query, now));
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

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        });
        results.truncate(24);
        if results.is_empty() {
            results.push(no_results_item(&query));
        }
        results
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

    fn default_results(&self, mode: SearchMode, now: u64) -> Vec<ResultItem> {
        let mut results = Vec::new();

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
                        return Some(WindowFocusTarget::Hyprland { address });
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
        WindowFocusTarget::Hyprland { address } => (
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
                focus_target: WindowFocusTarget::Hyprland { address },
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
        AppEntry, FileSearchBackend, Sources, no_results_item, parse_file_search_line,
        parse_hypr_windows_json, parse_niri_windows_json, pass_entry_name, window_focus_command,
    };
    use crate::model::{Action, PasswordOperation, QueryInput, SearchMode, WindowFocusTarget};
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
                address: "0xabc".to_string()
            }
        );
    }

    #[test]
    fn builds_native_focus_command_for_hyprland_window() {
        let (program, args) = window_focus_command(&WindowFocusTarget::Hyprland {
            address: "0xabc".to_string(),
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
        let sources = Sources {
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: false,
            qalc_available: false,
            predictions: empty_prediction_store(),
        };

        let results = sources.search("unlikely-query", SearchMode::Apps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "Status");
        assert!(matches!(results[0].action, Action::None));
    }

    #[test]
    fn bare_urls_surface_as_browser_results_in_all_mode() {
        let sources = Sources {
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: false,
            qalc_available: false,
            predictions: empty_prediction_store(),
        };

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
            text: "report".to_string(),
        });

        assert_eq!(item.title, "No matches for \"report\"");
        assert!(item.subtitle.contains("file indexer"));
        assert!(matches!(item.action, Action::None));
    }

    #[test]
    fn file_mode_requires_a_minimum_query_length_before_shelling_out() {
        let sources = Sources {
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            file_search_backend: Some(FileSearchBackend::LocalSearch),
            pass_available: false,
            qalc_available: false,
            predictions: empty_prediction_store(),
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
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: vec![super::PassEntry {
                name: "github/work".to_string(),
                search_blob: "github/work".to_string(),
            }],
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: true,
            qalc_available: false,
            predictions: empty_prediction_store(),
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
            apps: Vec::new(),
            ssh_hosts: Vec::new(),
            pass_entries: vec![super::PassEntry {
                name: "github/work".to_string(),
                search_blob: "github/work".to_string(),
            }],
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: true,
            qalc_available: false,
            predictions: empty_prediction_store(),
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
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: false,
            qalc_available: false,
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
            ssh_hosts: Vec::new(),
            pass_entries: Vec::new(),
            commands: Vec::new(),
            file_search_backend: None,
            pass_available: false,
            qalc_available: false,
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
        };

        let results = sources.search("", SearchMode::All);

        assert_eq!(results[0].source, "Commands");
        assert_eq!(results[0].title, "Run \"git status\"");
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
    let subtitle = match query.mode {
        SearchMode::All => "Try a broader term or switch to a dedicated mode.".to_string(),
        SearchMode::Apps => "Try a different app name or executable.".to_string(),
        SearchMode::Windows => "Try a window title, app id, or workspace name.".to_string(),
        SearchMode::Files => {
            "Try a different file name or ensure the file indexer has indexed it.".to_string()
        }
        SearchMode::Ssh => "Check ~/.ssh/config and known_hosts for the expected host.".to_string(),
        SearchMode::Pass => "Try a different password-store entry name.".to_string(),
        SearchMode::Commands => {
            "Try a different executable name or a full shell command.".to_string()
        }
        SearchMode::Web => "Press Enter to open a browser search result instead.".to_string(),
        SearchMode::Calc => "Try a valid libqalculate expression such as 42/7.".to_string(),
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
