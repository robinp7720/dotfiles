use crate::model::{Action, QueryInput, ResultItem, SearchMode, browser_target, score_text};
use gtk4::gio;
use gtk4::prelude::*;
use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

const MAX_APPS: usize = 8;
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
pub struct Sources {
    apps: Vec<AppEntry>,
    ssh_hosts: Vec<String>,
    pass_entries: Vec<PassEntry>,
    commands: Vec<String>,
    file_search_backend: Option<FileSearchBackend>,
    pass_available: bool,
    qalc_available: bool,
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

        if query.mode.includes(SearchMode::Pass) {
            results.extend(self.search_pass(&query));
        }

        if query.mode == SearchMode::Commands {
            results.extend(self.search_commands(&query));
        }

        if query.mode.includes(SearchMode::Calc) {
            if let Some(result) = self.search_calc(&query) {
                results.push(result);
            }
        }

        if let Some(result) = self.search_url(&query) {
            results.push(result);
        }

        if query.mode == SearchMode::Web {
            results.push(self.search_web(&query));
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
                    "Type an entry name and press Enter to copy its password",
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

    fn search_pass(&self, query: &QueryInput) -> Vec<ResultItem> {
        if !self.pass_available {
            if query.mode == SearchMode::Pass {
                return vec![ResultItem {
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

        let mut items = self
            .pass_entries
            .iter()
            .filter_map(|entry| {
                let score = score_text(&entry.search_blob, &query.text)?;
                Some(ResultItem {
                    title: entry.name.clone(),
                    subtitle: "Copy the first line from pass show".to_string(),
                    source: "Passwords",
                    icon_name: "dialog-password-symbolic".to_string(),
                    score: score + 880,
                    action: Action::CopyPass {
                        entry: entry.name.clone(),
                    },
                })
            })
            .collect::<Vec<_>>();

        sort_results(&mut items, MAX_PASS);
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

    fn search_url(&self, query: &QueryInput) -> Option<ResultItem> {
        if !matches!(query.mode, SearchMode::All | SearchMode::Web) {
            return None;
        }

        let url = browser_target(&query.text)?;
        Some(ResultItem {
            title: format!("Open {url}"),
            subtitle: "Open URL in the default browser".to_string(),
            source: "Web",
            icon_name: "web-browser-symbolic".to_string(),
            score: 1_200,
            action: Action::OpenUrl { url },
        })
    }

    fn search_web(&self, query: &QueryInput) -> ResultItem {
        ResultItem {
            title: format!("Search the web for “{}”", query.text),
            subtitle: "Open the default browser".to_string(),
            source: "Web",
            icon_name: "web-browser-symbolic".to_string(),
            score: 120,
            action: Action::WebSearch {
                query: query.text.clone(),
            },
        }
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
        FileSearchBackend, Sources, no_results_item, parse_file_search_line, pass_entry_name,
    };
    use crate::model::{Action, QueryInput, SearchMode};
    use std::path::Path;

    #[test]
    fn indexed_paths_are_uri_decoded() {
        let line = "file:///tmp/with%20space%23hash.txt";
        assert_eq!(
            parse_file_search_line(line).as_deref(),
            Some("/tmp/with space#hash.txt")
        );
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
        };

        let results = sources.search("/ a", SearchMode::All);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "Files");
        assert_eq!(results[0].title, "Keep typing to search files");
        assert!(matches!(results[0].action, Action::None));
    }

    #[test]
    fn pass_entries_are_searchable_and_copyable() {
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
        };

        let results = sources.search("pass: github", SearchMode::All);
        assert!(matches!(
            results.first().map(|item| &item.action),
            Some(Action::CopyPass { entry }) if entry == "github/work"
        ));
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
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        source,
        icon_name: icon_name.to_string(),
        score,
        action: Action::None,
    }
}
