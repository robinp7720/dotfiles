use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::{Command, Stdio};

const DEFAULT_CLIP_TIMEOUT_SECONDS: u64 = 15;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Credential {
    pub entry: String,
    pub password: String,
    pub username: String,
    pub fields: HashMap<String, String>,
    pub url: Option<String>,
    pub otp_uri: Option<String>,
    pub autotype: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeStep {
    Text(String),
    Key(&'static str),
    Delay(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramInput {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
}

pub fn parse_credential(entry: &str, raw: &str) -> Result<Credential> {
    let mut lines = raw.lines();
    let password = lines
        .next()
        .map(str::to_string)
        .filter(|line| !line.is_empty())
        .context("pass entry did not contain a password on the first line")?;
    let mut fields = HashMap::new();
    let mut otp_uri = None;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("otpauth://") && otp_uri.is_none() {
            otp_uri = Some(trimmed.to_string());
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() || fields.contains_key(&key) {
            continue;
        }
        fields.insert(key, value.trim().to_string());
    }

    let username = ["user", "username", "email"]
        .iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_username(entry));

    let url = ["url", "website"]
        .iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.is_empty());
    let autotype = ["autotype", "type"]
        .iter()
        .find_map(|key| fields.get(*key).cloned())
        .filter(|value| !value.is_empty());

    Ok(Credential {
        entry: entry.to_string(),
        password,
        username,
        fields,
        url,
        otp_uri,
        autotype,
    })
}

pub fn fallback_username(entry: &str) -> String {
    entry
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(entry)
        .to_string()
}

pub fn default_login_steps(credential: &Credential) -> Vec<TypeStep> {
    vec![
        TypeStep::Text(credential.username.clone()),
        TypeStep::Key("Tab"),
        TypeStep::Text(credential.password.clone()),
    ]
}

pub fn wtype_commands_for_steps(steps: &[TypeStep]) -> Vec<ProgramInput> {
    steps
        .iter()
        .map(|step| match step {
            TypeStep::Text(text) => ProgramInput {
                program: "wtype".to_string(),
                args: vec!["-".to_string()],
                stdin: Some(text.clone()),
            },
            TypeStep::Key(key) => ProgramInput {
                program: "wtype".to_string(),
                args: vec!["-k".to_string(), (*key).to_string()],
                stdin: None,
            },
            TypeStep::Delay(ms) => ProgramInput {
                program: "wtype".to_string(),
                args: vec!["-s".to_string(), ms.to_string()],
                stdin: None,
            },
        })
        .collect()
}

pub fn xdotool_commands_for_steps(steps: &[TypeStep]) -> Vec<ProgramInput> {
    steps
        .iter()
        .map(|step| match step {
            TypeStep::Text(text) => ProgramInput {
                program: "xdotool".to_string(),
                args: vec![
                    "type".to_string(),
                    "--clearmodifiers".to_string(),
                    "--file".to_string(),
                    "-".to_string(),
                ],
                stdin: Some(text.clone()),
            },
            TypeStep::Key(key) => ProgramInput {
                program: "xdotool".to_string(),
                args: vec!["key".to_string(), (*key).to_string()],
                stdin: None,
            },
            TypeStep::Delay(ms) => ProgramInput {
                program: "sleep".to_string(),
                args: vec![(f64::from(*ms as u32) / 1000.0).to_string()],
                stdin: None,
            },
        })
        .collect()
}

pub fn wl_copy_command(text: &str, timeout_seconds: u64) -> ProgramInput {
    let timeout = if timeout_seconds == 0 {
        DEFAULT_CLIP_TIMEOUT_SECONDS
    } else {
        timeout_seconds
    };

    ProgramInput {
        program: "wl-copy".to_string(),
        args: vec![
            "--trim-newline".to_string(),
            "--paste-once".to_string(),
            "--clear".to_string(),
            "--expire".to_string(),
            timeout.to_string(),
        ],
        stdin: Some(text.to_string()),
    }
}

pub fn xclip_command(text: &str) -> ProgramInput {
    ProgramInput {
        program: "xclip".to_string(),
        args: vec![
            "-selection".to_string(),
            "clipboard".to_string(),
            "-in".to_string(),
        ],
        stdin: Some(text.to_string()),
    }
}

pub fn run_program_input(input: ProgramInput) -> Result<()> {
    let program = input.program;
    let mut child = Command::new(&program)
        .args(&input.args)
        .stdin(if input.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;

    if let Some(stdin) = input.stdin {
        use std::io::Write;
        let mut pipe = child.stdin.take().context("failed to open child stdin")?;
        pipe.write_all(stdin.as_bytes())
            .with_context(|| format!("failed to write to {program}"))?;
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for {program}"))?;
    if !status.success() {
        anyhow::bail!("{program} failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ProgramInput, TypeStep, default_login_steps, parse_credential, run_program_input,
        wl_copy_command, wtype_commands_for_steps, xclip_command,
    };
    use std::fs;

    #[test]
    fn parses_standard_pass_entry_with_username_url_and_otp() {
        let credential = parse_credential(
            "github/work",
            "secret\nusername: robin\nurl: https://github.com\notpauth://totp/GitHub:robin?secret=ABC\n",
        )
        .expect("credential");

        assert_eq!(credential.password, "secret");
        assert_eq!(credential.username, "robin");
        assert_eq!(credential.url.as_deref(), Some("https://github.com"));
        assert!(
            credential
                .otp_uri
                .as_deref()
                .unwrap()
                .starts_with("otpauth://")
        );
    }

    #[test]
    fn username_falls_back_to_entry_basename() {
        let credential = parse_credential("servers/prometheus", "secret\n")
            .expect("credential with fallback username");

        assert_eq!(credential.username, "prometheus");
    }

    #[test]
    fn default_login_autotype_never_submits() {
        let credential = parse_credential("github/work", "secret\nemail: robin@example.com\n")
            .expect("credential");

        assert_eq!(
            default_login_steps(&credential),
            vec![
                TypeStep::Text("robin@example.com".to_string()),
                TypeStep::Key("Tab"),
                TypeStep::Text("secret".to_string()),
            ]
        );
    }

    #[test]
    fn wtype_commands_keep_secret_text_out_of_argv() {
        let commands = wtype_commands_for_steps(&[
            TypeStep::Text("robin".to_string()),
            TypeStep::Key("Tab"),
            TypeStep::Text("secret".to_string()),
        ]);

        assert_eq!(commands.len(), 3);
        assert!(
            commands
                .iter()
                .flat_map(|command| command.args.iter())
                .all(|arg| !arg.contains("secret"))
        );
        assert_eq!(commands[0].stdin.as_deref(), Some("robin"));
        assert_eq!(commands[2].stdin.as_deref(), Some("secret"));
    }

    #[test]
    fn clipboard_command_keeps_secret_text_out_of_argv() {
        let command = wl_copy_command("secret", 15);

        assert_eq!(command.program, "wl-copy");
        assert!(!command.args.iter().any(|arg| arg.contains("secret")));
        assert_eq!(command.stdin.as_deref(), Some("secret"));
    }

    #[test]
    fn xclip_command_keeps_secret_text_out_of_argv() {
        let command = xclip_command("secret");

        assert_eq!(command.program, "xclip");
        assert!(!command.args.iter().any(|arg| arg.contains("secret")));
        assert_eq!(command.stdin.as_deref(), Some("secret"));
    }

    #[test]
    fn program_runner_writes_stdin_to_child() {
        let temp_dir = std::env::temp_dir().join(format!(
            "dot-launcher-program-input-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let output_path = temp_dir.join("stdin.txt");
        let script_path = temp_dir.join("capture-stdin.sh");
        fs::write(
            &script_path,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\ncat > {}\n",
                shell_quote(output_path.to_string_lossy().as_ref())
            ),
        )
        .expect("write fake child");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions).expect("chmod fake child");
        }

        run_program_input(ProgramInput {
            program: script_path.to_string_lossy().to_string(),
            args: Vec::new(),
            stdin: Some("secret".to_string()),
        })
        .expect("run fake child");

        assert_eq!(
            fs::read_to_string(&output_path).expect("read stdin"),
            "secret"
        );
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
