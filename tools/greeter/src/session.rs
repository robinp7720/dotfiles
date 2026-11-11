use anyhow::{Result, bail, ensure};
use gtk4::glib;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub argv: Vec<String>,
    pub desktop: String,
    pub x11: bool,
}
impl Session {
    pub fn command(&self) -> Vec<String> {
        if self.x11 {
            [
                vec!["/usr/bin/startx".into(), "/usr/bin/env".into()],
                self.argv.clone(),
            ]
            .concat()
        } else {
            self.argv.clone()
        }
    }
    pub fn environment(&self) -> Vec<String> {
        let stem = self
            .id
            .split_once(':')
            .map(|(_, s)| s)
            .unwrap_or(&self.id)
            .trim_end_matches(".desktop");
        vec![
            format!(
                "XDG_SESSION_TYPE={}",
                if self.x11 { "x11" } else { "wayland" }
            ),
            format!(
                "XDG_CURRENT_DESKTOP={}",
                self.desktop.replace(';', ":").trim_end_matches(':')
            ),
            format!("XDG_SESSION_DESKTOP={stem}"),
            format!("DESKTOP_SESSION={stem}"),
        ]
    }
}

// Desktop Exec is not a shell command. Values have already been unescaped by GKeyFile.
fn exec_words(exec: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' => {
                let next = chars
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Trailing escape"))?;
                ensure!(
                    quoted && ['"', '`', '$', '\\'].contains(&next),
                    "Invalid Exec escape"
                );
                word.push(next);
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                ensure!(
                    quoted
                        || ![
                            '\'', '`', '$', '>', '<', '~', '|', '&', ';', '*', '?', '#', '(', ')'
                        ]
                        .contains(&c),
                    "Unquoted reserved Exec character"
                );
                word.push(c);
                started = true;
            }
        }
    }
    ensure!(!quoted, "Unclosed Exec quote");
    if started {
        words.push(word);
    }
    ensure!(
        !words.is_empty() && !words[0].is_empty(),
        "Missing executable"
    );
    Ok(words)
}
fn expand_exec(exec: &str, name: &str, file: &Path, icon: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for word in exec_words(exec)? {
        if ["%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%v", "%m"].contains(&word.as_str()) {
            continue;
        }
        if word == "%i" {
            if !icon.is_empty() {
                result.extend(["--icon".into(), icon.into()]);
            }
            continue;
        }
        let mut expanded = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                expanded.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('c') => expanded.push_str(name),
                Some('k') => expanded.push_str(&file.to_string_lossy()),
                _ => bail!("Unsupported or embedded Exec field code"),
            }
        }
        result.push(expanded);
    }
    ensure!(
        !result.is_empty() && !result[0].is_empty() && !result[0].contains('='),
        "Invalid executable"
    );
    Ok(result)
}
fn executable(command: &str) -> bool {
    glib::find_program_in_path(command).is_some()
}

pub fn read_entry(path: &Path, x11: bool) -> Result<Session> {
    let key = glib::KeyFile::new();
    key.load_from_file(path, glib::KeyFileFlags::NONE)?;
    let get = |name| {
        key.string("Desktop Entry", name)
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    ensure!(
        !key.boolean("Desktop Entry", "Hidden").unwrap_or(false)
            && !key.boolean("Desktop Entry", "NoDisplay").unwrap_or(false),
        "Hidden session"
    );
    let name = key
        .locale_string("Desktop Entry", "Name", None)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| get("Name"));
    ensure!(!name.is_empty(), "Missing session name");
    let try_exec = get("TryExec");
    ensure!(
        try_exec.is_empty() || executable(&try_exec),
        "TryExec unavailable"
    );
    let argv = expand_exec(&get("Exec"), &name, path, &get("Icon"))?;
    ensure!(executable(&argv[0]), "Executable unavailable");
    ensure!(
        !x11 || (executable("startx") && executable("Xorg")),
        "X11 unavailable"
    );
    let id = format!(
        "{}:{}",
        if x11 { "x11" } else { "wayland" },
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    Ok(Session {
        id,
        name,
        argv,
        desktop: get("DesktopNames"),
        x11,
    })
}
pub fn discover(dirs: &[PathBuf]) -> Vec<Session> {
    let mut sessions = Vec::new();
    for dir in dirs {
        let x11 = dir.file_name().is_some_and(|name| name == "xsessions");
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.extension().is_some_and(|ext| ext == "desktop")
                && let Ok(session) = read_entry(&path, x11)
                && !sessions.iter().any(|s: &Session| s.id == session.id)
            {
                sessions.push(session);
            }
        }
    }
    sessions.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    sessions
}
pub fn selected(sessions: &[Session], remembered: Option<&str>, initial: &str) -> Option<usize> {
    remembered
        .and_then(|id| sessions.iter().position(|s| s.id == id))
        .or_else(|| sessions.iter().position(|s| s.id == initial))
        .or(if sessions.is_empty() { None } else { Some(0) })
}
pub fn users() -> Vec<String> {
    let minimum = fs::read_to_string("/etc/login.defs")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == "UID_MIN")
                .then(|| fields.next()?.parse::<u32>().ok())
                .flatten()
        })
        .unwrap_or(1000);
    let output = Command::new("getent").arg("passwd").output();
    let mut users: Vec<String> = output
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    let parts: Vec<_> = line.split(':').collect();
                    if parts.len() != 7 {
                        return None;
                    }
                    let uid = parts[2].parse::<u32>().ok()?;
                    (uid >= minimum
                        && uid < 65534
                        && !parts[6].ends_with("nologin")
                        && !parts[6].ends_with("false"))
                    .then(|| parts[0].to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    users.sort();
    users.dedup();
    users
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoting_and_expansion_do_not_invoke_a_shell() {
        let argv = expand_exec(
            r#"/bin/echo "two words" "literal $HOME" %% %c %k %f %i"#,
            "My desktop",
            Path::new("/a b.desktop"),
            "icon",
        )
        .unwrap();
        assert_eq!(
            argv,
            [
                "/bin/echo",
                "two words",
                "literal $HOME",
                "%",
                "My desktop",
                "/a b.desktop",
                "--icon",
                "icon"
            ]
        );
        for bad in [
            "echo $(id)",
            "echo x;id",
            "echo %Z",
            "echo \"unfinished",
            "echo %Ftail",
        ] {
            assert!(expand_exec(bad, "", Path::new(""), "").is_err(), "{bad}");
        }
    }
    #[test]
    fn discovery_filters_entries_and_preserves_session_type() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("good.desktop"),
            "[Desktop Entry]\nName=Test\nExec=/bin/echo \"two words\"\nDesktopNames=Example;\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("hidden.desktop"),
            "[Desktop Entry]\nName=Hidden\nExec=/bin/true\nHidden=true\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("missing.desktop"),
            "[Desktop Entry]\nName=Missing\nExec=/no/such/binary\n",
        )
        .unwrap();
        let sessions = discover(&[tmp.path().into()]);
        assert_eq!(sessions.len(), 1);
        let mut s = sessions[0].clone();
        assert_eq!(s.argv, ["/bin/echo", "two words"]);
        s.x11 = true;
        assert_eq!(
            s.command(),
            ["/usr/bin/startx", "/usr/bin/env", "/bin/echo", "two words"]
        );
        assert!(s.environment().contains(&"XDG_SESSION_TYPE=x11".into()));
        assert!(
            !s.environment()
                .iter()
                .any(|e| e.starts_with("WAYLAND_DISPLAY=") || e.starts_with("DISPLAY="))
        );
        assert_eq!(selected(&sessions, Some("gone"), "gone"), Some(0));
        assert_eq!(selected(&[], None, ""), None);
    }
}
