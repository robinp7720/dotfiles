use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SearchMode {
    All,
    Apps,
    Files,
    Ssh,
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
            SearchMode::Files => "Files",
            SearchMode::Ssh => "SSH",
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
}

#[derive(Clone, Debug)]
pub enum Action {
    LaunchApp { desktop_id: String },
    OpenFile { path: String },
    Ssh { host: String },
    RunCommand { command: String },
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
        '@' => return Some((SearchMode::Ssh, rest.trim_start())),
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
    const PREFIXES: [(&str, SearchMode); 9] = [
        ("apps:", SearchMode::Apps),
        ("app:", SearchMode::Apps),
        ("files:", SearchMode::Files),
        ("file:", SearchMode::Files),
        ("ssh:", SearchMode::Ssh),
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

#[cfg(test)]
mod tests {
    use super::{QueryInput, SearchMode};

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
}
