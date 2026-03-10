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
        Self {
            mode: cli_mode,
            text: raw.trim().to_string(),
        }
    }
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
