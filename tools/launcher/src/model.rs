use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SearchMode {
    All,
    Apps,
    Windows,
    Files,
    Ssh,
    Pass,
    Commands,
    Web,
    Calc,
}

impl SearchMode {
    pub fn includes(self, other: SearchMode) -> bool {
        matches!(self, SearchMode::All) || self == other
    }

    pub fn label(self) -> &'static str {
        match self {
            SearchMode::All => "All",
            SearchMode::Apps => "Applications",
            SearchMode::Windows => "Windows",
            SearchMode::Files => "Files",
            SearchMode::Ssh => "SSH",
            SearchMode::Pass => "Passwords",
            SearchMode::Commands => "Commands",
            SearchMode::Web => "Web",
            SearchMode::Calc => "Calculator",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResultItem {
    pub title: String,
    pub subtitle: String,
    pub source: &'static str,
    pub icon_name: String,
    pub score: i32,
    pub action: Action,
    pub prediction_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WindowFocusTarget {
    Hyprland { address: String },
    Niri { id: u64 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Action {
    LaunchApp { desktop_id: String },
    FocusWindow { target: WindowFocusTarget },
    OpenFile { path: String },
    Ssh { host: String },
    CopyPass { entry: String },
    RunCommand { command: String },
    OpenUrl { url: String },
    WebSearch { query: String },
    CopyText { text: String },
    None,
}

#[derive(Clone, Debug)]
pub struct QueryInput {
    pub mode: SearchMode,
    pub text: String,
}

impl QueryInput {
    pub fn parse(raw: &str, cli_mode: SearchMode) -> Self {
        let trimmed = raw.trim();
        let (mode, text) = parse_prefixed_query(trimmed).unwrap_or((cli_mode, trimmed));
        Self {
            mode,
            text: text.to_string(),
        }
    }
}

fn parse_prefixed_query(raw: &str) -> Option<(SearchMode, &str)> {
    if raw.is_empty() {
        return None;
    }

    let mut chars = raw.chars();
    let first = chars.next()?;
    let rest = &raw[first.len_utf8()..];

    match first {
        '>' => return Some((SearchMode::Commands, rest.trim_start())),
        '~' => return Some((SearchMode::Windows, rest.trim_start())),
        '@' => return Some((SearchMode::Ssh, rest.trim_start())),
        '!' => return Some((SearchMode::Pass, rest.trim_start())),
        '?' => return Some((SearchMode::Web, rest.trim_start())),
        '=' => return Some((SearchMode::Calc, rest.trim_start())),
        '/' => {
            let whitespace_prefixed = rest.chars().next().is_none_or(char::is_whitespace);
            if whitespace_prefixed {
                return Some((SearchMode::Files, rest.trim_start()));
            }
        }
        _ => {}
    }

    let lowered = raw.to_ascii_lowercase();
    const PREFIXES: [(&str, SearchMode); 14] = [
        ("apps:", SearchMode::Apps),
        ("app:", SearchMode::Apps),
        ("windows:", SearchMode::Windows),
        ("window:", SearchMode::Windows),
        ("win:", SearchMode::Windows),
        ("files:", SearchMode::Files),
        ("file:", SearchMode::Files),
        ("ssh:", SearchMode::Ssh),
        ("pass:", SearchMode::Pass),
        ("password:", SearchMode::Pass),
        ("cmd:", SearchMode::Commands),
        ("command:", SearchMode::Commands),
        ("web:", SearchMode::Web),
        ("calc:", SearchMode::Calc),
    ];

    PREFIXES.iter().find_map(|(prefix, mode)| {
        lowered
            .strip_prefix(prefix)
            .map(|_| (*mode, raw[prefix.len()..].trim_start()))
    })
}

pub fn score_text(haystack: &str, query: &str) -> Option<i32> {
    let haystack = haystack.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();

    if query.is_empty() {
        return Some(0);
    }

    if haystack == query {
        return Some(1_000);
    }

    if let Some(rest) = haystack.strip_prefix(&query) {
        return Some(850 - rest.len() as i32);
    }

    if let Some(position) = haystack.find(&query) {
        return Some(600 - position as i32);
    }

    if is_subsequence(&haystack, &query) {
        return Some(400 - (haystack.len() as i32 - query.len() as i32));
    }

    None
}

fn is_subsequence(haystack: &str, needle: &str) -> bool {
    let mut chars = needle.chars();
    let mut current = chars.next();

    for ch in haystack.chars() {
        if current == Some(ch) {
            current = chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    current.is_none()
}

pub fn browser_target(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    if has_uri_scheme(trimmed) {
        return Some(trimmed.to_string());
    }

    if trimmed.starts_with("www.") && looks_like_web_host(trimmed) {
        return Some(format!("https://{trimmed}"));
    }

    if looks_like_web_host(trimmed) {
        return Some(format!("https://{trimmed}"));
    }

    None
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };

    !scheme.is_empty()
        && !rest.is_empty()
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn looks_like_web_host(value: &str) -> bool {
    let authority = value
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');

    if authority.is_empty() {
        return false;
    }

    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(host, _)| host)
        .unwrap_or(host);

    if host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }

    if !host.contains('.') {
        return false;
    }

    host.split('.').all(valid_domain_label)
}

fn valid_domain_label(label: &str) -> bool {
    !label.is_empty()
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::{QueryInput, SearchMode, browser_target};

    #[test]
    fn symbol_prefixes_override_the_default_mode() {
        let query = QueryInput::parse("> git status", SearchMode::Apps);
        assert_eq!(query.mode, SearchMode::Commands);
        assert_eq!(query.text, "git status");
    }

    #[test]
    fn textual_prefixes_are_case_insensitive() {
        let query = QueryInput::parse("SSH: prod-box", SearchMode::All);
        assert_eq!(query.mode, SearchMode::Ssh);
        assert_eq!(query.text, "prod-box");
    }

    #[test]
    fn pass_prefixes_override_the_default_mode() {
        let symbol_prefixed = QueryInput::parse("! github/work", SearchMode::All);
        assert_eq!(symbol_prefixed.mode, SearchMode::Pass);
        assert_eq!(symbol_prefixed.text, "github/work");

        let text_prefixed = QueryInput::parse("PASS: github/work", SearchMode::Apps);
        assert_eq!(text_prefixed.mode, SearchMode::Pass);
        assert_eq!(text_prefixed.text, "github/work");
    }

    #[test]
    fn window_prefixes_override_the_default_mode() {
        let symbol_prefixed = QueryInput::parse("~ terminal", SearchMode::All);
        assert_eq!(symbol_prefixed.mode, SearchMode::Windows);
        assert_eq!(symbol_prefixed.text, "terminal");

        let text_prefixed = QueryInput::parse("windows: firefox", SearchMode::Apps);
        assert_eq!(text_prefixed.mode, SearchMode::Windows);
        assert_eq!(text_prefixed.text, "firefox");
    }

    #[test]
    fn empty_symbol_prefix_keeps_the_target_mode() {
        let query = QueryInput::parse("=", SearchMode::All);
        assert_eq!(query.mode, SearchMode::Calc);
        assert!(query.text.is_empty());
    }

    #[test]
    fn slash_without_whitespace_stays_a_plain_query() {
        let query = QueryInput::parse("/etc", SearchMode::All);
        assert_eq!(query.mode, SearchMode::All);
        assert_eq!(query.text, "/etc");
    }

    #[test]
    fn browser_target_recognizes_full_urls() {
        assert_eq!(
            browser_target("https://example.com/docs?q=1").as_deref(),
            Some("https://example.com/docs?q=1")
        );
    }

    #[test]
    fn browser_target_adds_https_for_bare_domains() {
        assert_eq!(
            browser_target("example.com/notes").as_deref(),
            Some("https://example.com/notes")
        );
    }

    #[test]
    fn browser_target_rejects_plain_search_terms() {
        assert_eq!(browser_target("firefox"), None);
        assert_eq!(browser_target("two words"), None);
    }
}
