use crate::model::{Action, QueryInput, ResultItem, SearchMode, score_text};
use gtk4::gio;
use gtk4::prelude::*;
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_APPS: usize = 8;
const MAX_FILES: usize = 8;
const MAX_SSH: usize = 6;
const MAX_COMMANDS: usize = 8;

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
pub struct Sources {
    apps: Vec<AppEntry>,
    ssh_hosts: Vec<String>,
    commands: Vec<String>,
    tracker3_available: bool,
    qalc_available: bool,
}

impl Sources {
    pub fn load() -> Self {
        Self {
            apps: load_applications(),
            ssh_hosts: load_ssh_hosts(),
            commands: load_commands(),
            tracker3_available: command_exists("tracker3"),
            qalc_available: command_exists("qalc"),
        }
    }

    pub fn search(&self, raw_query: &str, cli_mode: SearchMode) -> Vec<ResultItem> {
        let query = QueryInput::parse(raw_query, cli_mode);
        let mut results = Vec::new();

        if query.text.is_empty() {
            results.extend(self.default_results(query.mode));
            return results;
        }

        if query.mode.includes(SearchMode::Apps) {
            results.extend(self.search_apps(&query));
        }

        if query.mode.includes(SearchMode::Files) {
            results.extend(self.search_files(&query));
        }

        if query.mode.includes(SearchMode::Ssh) {
            results.extend(self.search_ssh(&query));
        }

        if query.mode.includes(SearchMode::Commands) {
            results.extend(self.search_commands(&query));
        }

        if query.mode.includes(SearchMode::Calc) {
            if let Some(result) = self.search_calc(&query) {
                results.push(result);
            }
        }

        if query.mode.includes(SearchMode::Web) {
            results.push(ResultItem {
                title: format!("Search the web for “{}”", query.text),
                subtitle: "Open the default browser".to_string(),
                source: "Web",
                icon_name: "web-browser-symbolic".to_string(),
                score: 120,
                action: Action::WebSearch {
                    query: query.text.clone(),
                },
            });
        }

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.cmp(&right.title))
        });
        results.truncate(24);
        results
    }

    fn default_results(&self, mode: SearchMode) -> Vec<ResultItem> {
        let mut results = Vec::new();

        if mode.includes(SearchMode::Apps) {
            results.extend(self.apps.iter().take(8).map(|app| ResultItem {
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

        if mode.includes(SearchMode::Ssh) {
            results.extend(self.ssh_hosts.iter().take(4).map(|host| ResultItem {
                title: host.clone(),
                subtitle: "Open an SSH session".to_string(),
                source: "SSH",
                icon_name: "network-server-symbolic".to_string(),
                score: 70,
                action: Action::Ssh { host: host.clone() },
            }));
        }

        if mode == SearchMode::Files && !self.tracker3_available {
            results.push(ResultItem {
                title: "tracker3 is not installed".to_string(),
                subtitle: "Install tracker3 to enable indexed file search".to_string(),
                source: "Files",
                icon_name: "system-search-symbolic".to_string(),
                score: 65,
                action: Action::None,
            });
        }

        results
    }

    fn search_apps(&self, query: &QueryInput) -> Vec<ResultItem> {
        let mut items = self
            .apps
            .iter()
            .filter_map(|app| {
                let score = score_text(&app.search_blob, &query.text)?;
                Some(ResultItem {
                    title: app.name.clone(),
                    subtitle: if app.description.is_empty() {
                        app.executable.clone()
                    } else {
                        app.description.clone()
                    },
                    source: "Applications",
                    icon_name: app.icon_name.clone(),
                    score: score + 900,
                    action: Action::LaunchApp {
                        desktop_id: app.desktop_id.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_APPS);
        items
    }

    fn search_files(&self, query: &QueryInput) -> Vec<ResultItem> {
        if !self.tracker3_available {
            if query.mode == SearchMode::Files {
                return vec![ResultItem {
                    title: "tracker3 is not installed".to_string(),
                    subtitle: "Install tracker3 to enable indexed file search".to_string(),
                    source: "Files",
                    icon_name: "system-search-symbolic".to_string(),
                    score: 500,
                    action: Action::None,
                }];
            }
            return Vec::new();
        }

        let Ok(output) = Command::new("tracker3")
            .args(["search", "--limit", &MAX_FILES.to_string(), &query.text])
            .output()
        else {
            return Vec::new();
        };

        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(parse_tracker_line)
            .take(MAX_FILES)
            .map(|path| {
                let file_name = Path::new(&path)
                    .file_name()
                    .and_then(|part| part.to_str())
                    .unwrap_or(path.as_str())
                    .to_string();
                ResultItem {
                    title: file_name,
                    subtitle: path.clone(),
                    source: "Files",
                    icon_name: "folder-documents-symbolic".to_string(),
                    score: 760,
                    action: Action::OpenFile { path },
                }
            })
            .collect()
    }

    fn search_ssh(&self, query: &QueryInput) -> Vec<ResultItem> {
        let mut items = self
            .ssh_hosts
            .iter()
            .filter_map(|host| {
                let score = score_text(host, &query.text)?;
                Some(ResultItem {
                    title: host.clone(),
                    subtitle: "Open an SSH session".to_string(),
                    source: "SSH",
                    icon_name: "network-server-symbolic".to_string(),
                    score: score + 720,
                    action: Action::Ssh { host: host.clone() },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_SSH);
        items
    }

    fn search_commands(&self, query: &QueryInput) -> Vec<ResultItem> {
        let mut items = Vec::new();
        items.push(ResultItem {
            title: format!("Run “{}”", query.text),
            subtitle: "Execute in the background with sh -lc".to_string(),
            source: "Commands",
            icon_name: "utilities-terminal-symbolic".to_string(),
            score: 930,
            action: Action::RunCommand {
                command: query.text.clone(),
            },
        });

        let mut suggestions = self
            .commands
            .iter()
            .filter_map(|command| {
                let score = score_text(command, &query.text)?;
                Some(ResultItem {
                    title: command.clone(),
                    subtitle: "Executable from $PATH".to_string(),
                    source: "Commands",
                    icon_name: "utilities-terminal-symbolic".to_string(),
                    score: score + 700,
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

    fn search_calc(&self, query: &QueryInput) -> Option<ResultItem> {
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

        Some(ResultItem {
            title: result.clone(),
            subtitle: format!("Result for {}", query.text),
            source: "Calculator",
            icon_name: "accessories-calculator-symbolic".to_string(),
            score: 1_100,
            action: Action::CopyText { text: result },
        })
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

fn parse_tracker_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = trimmed
        .split_once(' ')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(trimmed);

    if let Some(path) = candidate.strip_prefix("file://") {
        let path = path.replace("%20", " ");
        return Some(path);
    }

    Some(candidate.to_string())
}

fn command_exists(binary: &str) -> bool {
    env::var_os("PATH")
        .unwrap_or_default()
        .to_string_lossy()
        .split(':')
        .map(Path::new)
        .any(|dir| dir.join(binary).exists())
}

fn looks_like_math(query: &str) -> bool {
    query
        .chars()
        .any(|ch| ch.is_ascii_digit() || "+-*/^=()".contains(ch))
        || query.contains(" to ")
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
