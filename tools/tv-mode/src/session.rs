use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{Config, DisplayMode};

const SINK_WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionSnapshot {
    pub version: u8,
    pub hyprland_instance: String,
    pub monitors: Vec<MonitorSnapshot>,
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub focused_monitor: Option<String>,
    pub focused_workspace: Option<String>,
    pub default_sink: String,
    pub audio_card_profile: String,
    pub power_profile: Option<String>,
    pub hypridle_active: bool,
    #[serde(default)]
    pub vigil_presentation_hidden: Option<bool>,
    #[serde(default)]
    pub vigil_dnd: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MonitorSnapshot {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: u8,
    #[serde(default)]
    pub active_workspace: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceSnapshot {
    pub name: String,
    pub monitor: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Home,
    Gaming,
    Media,
}

pub struct Session {
    config: Config,
    runtime_dir: PathBuf,
    snapshot: SessionSnapshot,
    restored: bool,
}

impl Session {
    pub fn enter(mut config: Config) -> Result<Self> {
        let runtime_dir = runtime_dir()?;
        fs::create_dir_all(&runtime_dir)
            .with_context(|| format!("failed to create {}", runtime_dir.display()))?;
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))?;

        let state_path = runtime_dir.join("state.json");
        if state_path.exists() {
            bail!(
                "TV mode state already exists at {}; run tv-mode recover first",
                state_path.display()
            );
        }

        preflight(&mut config)?;
        let snapshot = capture_snapshot(&config)?;
        write_snapshot(&state_path, &snapshot)?;

        let mut session = Self {
            config,
            runtime_dir,
            snapshot,
            restored: false,
        };
        if let Err(error) = session.apply_enter() {
            let rollback = session.restore();
            return match rollback {
                Ok(()) => Err(error.context("TV mode activation failed and was rolled back")),
                Err(rollback_error) => Err(error.context(format!(
                    "TV mode activation failed; rollback also failed: {rollback_error:#}"
                ))),
            };
        }
        Ok(session)
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn apply_profile(&self, profile: Profile) -> Result<()> {
        let (mode, power) = match profile {
            Profile::Home => (&self.config.modes.home, &self.config.power.home),
            Profile::Gaming => (&self.config.modes.gaming, &self.config.power.gaming),
            Profile::Media => (&self.config.modes.media, &self.config.power.media),
        };
        set_monitor_mode(&self.config, mode)?;
        set_power_profile(power)
    }

    pub fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        restore_snapshot(&self.config, &self.snapshot)?;
        let state_path = self.runtime_dir.join("state.json");
        if state_path.exists() {
            fs::remove_file(&state_path)
                .with_context(|| format!("failed to remove {}", state_path.display()))?;
        }
        self.restored = true;
        Ok(())
    }

    fn apply_enter(&mut self) -> Result<()> {
        try_vigil_action("hide presentation surfaces", set_vigil_presentation(true));
        try_vigil_action("enable DND", set_vigil_dnd(true));
        if self.snapshot.hypridle_active {
            run("systemctl", &["--user", "stop", "hypridle.service"])?;
        }
        switch_audio_to_tv(&self.config)?;
        set_power_profile(&self.config.power.home)?;
        run(
            "hyprctl",
            &[
                "dispatch",
                &format!("hl.dsp.focus({{ monitor = {:?} }})", self.config.output),
            ],
        )?;
        run(
            "hyprctl",
            &[
                "dispatch",
                &format!(
                    "hl.dsp.focus({{ workspace = {:?} }})",
                    format!("name:{}", self.config.workspace)
                ),
            ],
        )?;
        for monitor in &self.snapshot.monitors {
            if monitor.name != self.config.output {
                run(
                    "hyprctl",
                    &[
                        "eval",
                        &format!(
                            "hl.monitor({{ output = {:?}, disabled = true }})",
                            monitor.name
                        ),
                    ],
                )?;
            }
        }
        // Move the TV to the origin only after the desk outputs are disabled.
        set_monitor_mode(&self.config, &self.config.modes.home)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.restored
            && let Err(error) = self.restore()
        {
            eprintln!("failed to restore TV mode session during shutdown: {error:#}");
        }
    }
}

pub fn recover(config: &Config) -> Result<bool> {
    let state_path = state_path()?;
    if !state_path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&state_path)
        .with_context(|| format!("failed to read {}", state_path.display()))?;
    let snapshot: SessionSnapshot = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", state_path.display()))?;
    restore_snapshot(config, &snapshot)?;
    fs::remove_file(&state_path)
        .with_context(|| format!("failed to remove {}", state_path.display()))?;
    Ok(true)
}

pub fn is_active() -> Result<bool> {
    Ok(state_path()?.exists())
}

pub fn runtime_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("XDG_RUNTIME_DIR is not set"))?;
    Ok(base.join("tv-mode"))
}

pub fn state_path() -> Result<PathBuf> {
    Ok(runtime_dir()?.join("state.json"))
}

fn preflight(config: &mut Config) -> Result<()> {
    ensure!(
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        "TV mode requires a running Hyprland session"
    );
    for command in [
        "hyprctl",
        "pactl",
        "powerprofilesctl",
        "systemctl",
        "loginctl",
    ] {
        let status = Command::new("sh")
            .args(["-lc", &format!("command -v {command} >/dev/null")])
            .status()
            .with_context(|| format!("failed to check for {command}"))?;
        ensure!(
            status.success(),
            "required command {command} is unavailable"
        );
    }

    let monitors = hypr_json("monitors")?;
    let monitor = monitors
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| string(item, "name") == Some(config.output.as_str()))
        })
        .ok_or_else(|| anyhow!("TV output {} is not active", config.output))?;
    let available = monitor
        .get("availableModes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for mode in [
        &mut config.modes.home,
        &mut config.modes.gaming,
        &mut config.modes.media,
    ] {
        let resolved = resolve_mode(mode, &available).ok_or_else(|| {
            anyhow!(
                "TV output {} does not advertise mode {} (available: {})",
                config.output,
                mode,
                available
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        *mode = resolved.to_string();
    }

    let cards = output("pactl", &["list", "cards"])?;
    let cards = String::from_utf8_lossy(&cards.stdout);
    ensure!(
        cards.contains(&format!("Name: {}", config.audio.card)),
        "audio card {} is unavailable",
        config.audio.card
    );
    ensure!(
        cards.contains(&format!("{}:", config.audio.tv_profile)),
        "audio profile {} is unavailable on {}",
        config.audio.tv_profile,
        config.audio.card
    );
    Ok(())
}

fn resolve_mode<'a>(requested: &str, available: &'a [Value]) -> Option<&'a str> {
    let requested = DisplayMode::parse(requested)?;
    available
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|candidate| {
            let mode = DisplayMode::parse(candidate)?;
            let difference = (mode.refresh - requested.refresh).abs();
            // EDID modes often use fractional timings (29.97/59.94/119.88).
            // Keep the resolution exact and accept only near-equivalent refresh rates.
            (mode.width == requested.width
                && mode.height == requested.height
                && difference <= requested.refresh * 0.002)
                .then_some((candidate.trim_end_matches("Hz"), difference))
        })
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(mode, _)| mode)
}

fn capture_snapshot(config: &Config) -> Result<SessionSnapshot> {
    let monitors_json = hypr_json("monitors")?;
    let workspaces_json = hypr_json("workspaces")?;
    let monitors = monitors_json
        .as_array()
        .ok_or_else(|| anyhow!("hyprctl monitors did not return an array"))?
        .iter()
        .map(parse_monitor)
        .collect::<Result<Vec<_>>>()?;
    let focused_monitor = monitors_json
        .as_array()
        .and_then(|items| items.iter().find(|item| bool_field(item, "focused")))
        .and_then(|item| string(item, "name"))
        .map(str::to_string);
    let focused_workspace = monitors_json
        .as_array()
        .and_then(|items| items.iter().find(|item| bool_field(item, "focused")))
        .and_then(|item| item.get("activeWorkspace"))
        .and_then(|workspace| string(workspace, "name"))
        .map(str::to_string);
    let workspaces = workspaces_json
        .as_array()
        .ok_or_else(|| anyhow!("hyprctl workspaces did not return an array"))?
        .iter()
        .filter_map(|item| {
            let name = string(item, "name")?;
            let monitor = string(item, "monitor")?;
            (!name.starts_with("special:")).then(|| WorkspaceSnapshot {
                name: name.to_string(),
                monitor: monitor.to_string(),
            })
        })
        .collect();

    let default_sink = output_text("pactl", &["get-default-sink"])?;
    let audio_card_profile = active_card_profile(&config.audio.card)?;
    let power_profile = output_text("powerprofilesctl", &["get"]).ok();
    let hypridle_active = command_success(
        "systemctl",
        &["--user", "is-active", "--quiet", "hypridle.service"],
    );
    let vigil_presentation_hidden =
        optional_vigil_state("read presentation visibility", vigil_presentation_hidden());
    let vigil_dnd = optional_vigil_state("read DND state", vigil_dnd());

    Ok(SessionSnapshot {
        version: 1,
        hyprland_instance: std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap_or_default(),
        monitors,
        workspaces,
        focused_monitor,
        focused_workspace,
        default_sink,
        audio_card_profile,
        power_profile,
        hypridle_active,
        vigil_presentation_hidden,
        vigil_dnd,
    })
}

fn parse_monitor(value: &Value) -> Result<MonitorSnapshot> {
    Ok(MonitorSnapshot {
        name: required_string(value, "name")?,
        width: required_u64(value, "width")? as u32,
        height: required_u64(value, "height")? as u32,
        refresh_rate: value
            .get("refreshRate")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("monitor refreshRate is missing"))?,
        x: required_i64(value, "x")? as i32,
        y: required_i64(value, "y")? as i32,
        scale: value
            .get("scale")
            .and_then(Value::as_f64)
            .ok_or_else(|| anyhow!("monitor scale is missing"))?,
        transform: required_u64(value, "transform")? as u8,
        active_workspace: value
            .get("activeWorkspace")
            .and_then(|workspace| string(workspace, "name"))
            .map(str::to_string),
    })
}

fn restore_monitor_command(monitor: &MonitorSnapshot) -> String {
    let mode = format!(
        "{}x{}@{:.3}",
        monitor.width, monitor.height, monitor.refresh_rate
    );
    let position = format!("{}x{}", monitor.x, monitor.y);
    format!(
        "hl.monitor({{ output = {:?}, mode = {mode:?}, position = {position:?}, scale = {}, transform = {}, disabled = false }})",
        monitor.name, monitor.scale, monitor.transform
    )
}

fn restore_snapshot(config: &Config, snapshot: &SessionSnapshot) -> Result<()> {
    let mut errors = Vec::new();
    let same_hyprland_instance = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .is_some_and(|current| current == snapshot.hyprland_instance);

    if same_hyprland_instance {
        // Move the TV out of the desk outputs' space before re-enabling them.
        for monitor in snapshot
            .monitors
            .iter()
            .filter(|monitor| monitor.name == config.output)
            .chain(
                snapshot
                    .monitors
                    .iter()
                    .filter(|monitor| monitor.name != config.output),
            )
        {
            let command = restore_monitor_command(monitor);
            collect_error(&mut errors, run("hyprctl", &["eval", &command]));
        }
        thread::sleep(Duration::from_millis(350));
        let workspaces = match hypr_json("workspaces") {
            Ok(workspaces) => workspaces,
            Err(error) => {
                errors.push(error);
                Value::Null
            }
        };
        for workspace in &snapshot.workspaces {
            // Empty workspaces disappear when their output is disabled. Their
            // visible replacements are recreated from the monitor snapshots below.
            if !workspaces.as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| string(item, "name") == Some(workspace.name.as_str()))
            }) {
                continue;
            }
            collect_error(
                &mut errors,
                run(
                    "hyprctl",
                    &[
                        "dispatch",
                        &format!(
                            "hl.dsp.workspace.move({{ workspace = {:?}, monitor = {:?} }})",
                            workspace_selector(&workspace.name),
                            workspace.monitor
                        ),
                    ],
                ),
            );
        }
        for monitor in &snapshot.monitors {
            if let Some(workspace) = &monitor.active_workspace {
                collect_error(
                    &mut errors,
                    hyprland_dispatch(&format!("hl.dsp.focus({{ monitor = {:?} }})", monitor.name)),
                );
                collect_error(
                    &mut errors,
                    hyprland_dispatch(&format!(
                        "hl.dsp.focus({{ workspace = {:?} }})",
                        workspace_selector(workspace)
                    )),
                );
            }
        }
        if let Some(monitor) = &snapshot.focused_monitor {
            collect_error(
                &mut errors,
                run(
                    "hyprctl",
                    &[
                        "dispatch",
                        &format!("hl.dsp.focus({{ monitor = {monitor:?} }})"),
                    ],
                ),
            );
        }
        if let Some(workspace) = &snapshot.focused_workspace {
            collect_error(
                &mut errors,
                run(
                    "hyprctl",
                    &[
                        "dispatch",
                        &format!(
                            "hl.dsp.focus({{ workspace = {:?} }})",
                            workspace_selector(workspace)
                        ),
                    ],
                ),
            );
        }
    }

    collect_error(
        &mut errors,
        run(
            "pactl",
            &[
                "set-card-profile",
                &config.audio.card,
                &snapshot.audio_card_profile,
            ],
        ),
    );
    if let Err(error) = wait_for_sink(&snapshot.default_sink) {
        errors.push(error);
    } else {
        collect_error(
            &mut errors,
            run("pactl", &["set-default-sink", &snapshot.default_sink]),
        );
        collect_error(&mut errors, move_all_streams(&snapshot.default_sink));
    }
    if let Some(profile) = &snapshot.power_profile {
        collect_error(&mut errors, set_power_profile(profile));
    }
    if snapshot.hypridle_active {
        collect_error(
            &mut errors,
            run(
                "systemctl",
                &["--user", "start", "--no-block", "hypridle.service"],
            ),
        );
    }
    if let Some(enabled) = snapshot.vigil_dnd {
        try_vigil_action("restore DND state", set_vigil_dnd(enabled));
    }
    if let Some(hidden) = snapshot.vigil_presentation_hidden {
        try_vigil_action(
            "restore presentation visibility",
            set_vigil_presentation(hidden),
        );
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let details = errors
            .into_iter()
            .map(|error| format!("{error:#}"))
            .collect::<Vec<_>>()
            .join("; ");
        bail!("TV mode restoration was incomplete: {details}")
    }
}

fn workspace_selector(name: &str) -> String {
    if name.parse::<i32>().is_ok() {
        name.to_string()
    } else {
        format!("name:{name}")
    }
}

fn set_monitor_mode(config: &Config, mode: &str) -> Result<()> {
    run(
        "hyprctl",
        &[
            "eval",
            &format!(
                "hl.monitor({{ output = {:?}, mode = {mode:?}, position = \"0x0\", scale = {}, disabled = false }})",
                config.output, config.modes.scale
            ),
        ],
    )
}

fn switch_audio_to_tv(config: &Config) -> Result<()> {
    run(
        "pactl",
        &[
            "set-card-profile",
            &config.audio.card,
            &config.audio.tv_profile,
        ],
    )?;
    wait_for_sink(&config.audio.tv_sink)?;
    run("pactl", &["set-default-sink", &config.audio.tv_sink])?;
    move_all_streams(&config.audio.tv_sink)
}

fn move_all_streams(sink: &str) -> Result<()> {
    let list = output_text("pactl", &["list", "short", "sink-inputs"])?;
    for line in list.lines() {
        let Some(id) = line.split_whitespace().next() else {
            continue;
        };
        run("pactl", &["move-sink-input", id, sink])?;
    }
    Ok(())
}

fn wait_for_sink(sink: &str) -> Result<()> {
    let deadline = Instant::now() + SINK_WAIT;
    loop {
        let sinks = output_text("pactl", &["list", "short", "sinks"])?;
        if sinks
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(sink))
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for audio sink {sink}");
        }
        thread::sleep(Duration::from_millis(75));
    }
}

fn active_card_profile(card: &str) -> Result<String> {
    let cards = output_text("pactl", &["-f", "json", "list", "cards"])?;
    let cards: Value = serde_json::from_str(&cards).context("failed to parse pactl card JSON")?;
    cards
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(card))
        })
        .and_then(|item| item.get("active_profile"))
        .and_then(|profile| profile.get("name").or(Some(profile)))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("failed to determine active profile for audio card {card}"))
}

fn set_power_profile(profile: &str) -> Result<()> {
    run("powerprofilesctl", &["set", profile])
}

fn vigil_presentation_hidden() -> Result<bool> {
    match vigil::ControlClient::new()?.send(&vigil::ControlRequest::PresentationGet)? {
        vigil::ControlResponse::Presentation { hidden } => Ok(hidden),
        response => bail!("unexpected Vigil presentation response: {response:?}"),
    }
}

fn set_vigil_presentation(hidden: bool) -> Result<()> {
    match vigil::ControlClient::new()?.send(&vigil::ControlRequest::PresentationSet { hidden })? {
        vigil::ControlResponse::Accepted => Ok(()),
        vigil::ControlResponse::Error { message } => {
            bail!("Vigil rejected presentation: {message}")
        }
        response => bail!("unexpected Vigil presentation response: {response:?}"),
    }
}

fn vigil_dnd() -> Result<bool> {
    match vigil::ControlClient::new()?.send(&vigil::ControlRequest::NotificationsGet)? {
        vigil::ControlResponse::Notifications { state } => Ok(state.dnd),
        response => bail!("unexpected Vigil notification response: {response:?}"),
    }
}

fn set_vigil_dnd(enabled: bool) -> Result<()> {
    match vigil::ControlClient::new()?
        .send(&vigil::ControlRequest::NotificationsSetDnd { enabled })?
    {
        vigil::ControlResponse::Accepted => Ok(()),
        vigil::ControlResponse::Error { message } => bail!("Vigil rejected DND: {message}"),
        response => bail!("unexpected Vigil notification response: {response:?}"),
    }
}

fn optional_vigil_state<T>(action: &str, result: Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!(
                "warning: could not {action}; continuing without Vigil integration: {error:#}"
            );
            None
        }
    }
}

fn try_vigil_action(action: &str, result: Result<()>) {
    if let Err(error) = result {
        eprintln!("warning: could not {action}; continuing without Vigil integration: {error:#}");
    }
}

fn write_snapshot(path: &Path, snapshot: &SessionSnapshot) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("state path has no parent"))?;
    let temp = parent.join("state.json.tmp");
    let content = serde_json::to_vec_pretty(snapshot)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temp)
        .with_context(|| format!("failed to create {}", temp.display()))?;
    file.write_all(&content)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn hypr_json(command: &str) -> Result<Value> {
    let text = output_text("hyprctl", &[command, "-j"])?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse hyprctl {command} JSON"))
}

fn output(program: &str, args: &[&str]) -> Result<Output> {
    let result = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    check_output(program, args, &result)?;
    Ok(result)
}

fn check_output(program: &str, args: &[&str], result: &Output) -> Result<()> {
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    // Lua dispatcher warnings can be returned on stdout with exit status zero.
    let rejected = program == "hyprctl"
        && matches!(args.first(), Some(&"dispatch" | &"eval"))
        && !matches!(stdout.trim(), "" | "ok");
    if !result.status.success() || rejected {
        bail!(
            "{} {} failed: {}",
            program,
            args.join(" "),
            [stderr.trim(), stdout.trim()]
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    Ok(())
}

pub fn hyprland_dispatch(dispatcher: &str) -> Result<()> {
    run("hyprctl", &["dispatch", dispatcher])
}

fn output_text(program: &str, args: &[&str]) -> Result<String> {
    let result = output(program, args)?;
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    output(program, args).map(|_| ())
}

fn command_success(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

fn collect_error(errors: &mut Vec<anyhow::Error>, result: Result<()>) {
    if let Err(error) = result {
        errors.push(error);
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn bool_field(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    string(value, key)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("missing string field {key}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing integer field {key}"))
}

fn required_i64(value: &Value, key: &str) -> Result<i64> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("missing integer field {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn hyprland_warning_is_not_a_successful_dispatch() {
        let result = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"warning: hl.focus.monitor: monitor not found\n".to_vec(),
            stderr: Vec::new(),
        };
        let error = check_output("hyprctl", &["dispatch", "test"], &result).unwrap_err();
        assert!(error.to_string().contains("monitor not found"));
        let success = Output {
            stdout: b"ok\n".to_vec(),
            ..result
        };
        check_output("hyprctl", &["dispatch", "test"], &success).unwrap();
    }

    #[test]
    fn restores_named_workspaces_with_explicit_selectors() {
        assert_eq!(workspace_selector("4"), "4");
        assert_eq!(workspace_selector("games"), "name:games");
    }

    #[test]
    fn resolves_fractional_tv_timings_to_advertised_modes() {
        let available = vec![
            Value::from("3840x2160@29.97Hz"),
            Value::from("1920x1080@119.88Hz"),
        ];
        assert_eq!(
            resolve_mode("3840x2160@30.0", &available),
            Some("3840x2160@29.97")
        );
        assert_eq!(
            resolve_mode("1920x1080@120", &available),
            Some("1920x1080@119.88")
        );
        assert_eq!(
            resolve_mode("1920x1080@119.879", &available),
            Some("1920x1080@119.88")
        );
    }

    #[test]
    fn mode_resolution_prefers_closest_refresh_regardless_of_order() {
        let available = vec![
            Value::from("3840x2160@29.97Hz"),
            Value::from("3840x2160@30.00Hz"),
        ];
        assert_eq!(
            resolve_mode("3840x2160@30", &available),
            Some("3840x2160@30.00")
        );
    }

    #[test]
    fn mode_resolution_rejects_wrong_resolution_and_refresh_prefixes() {
        let available = vec![
            Value::from("3840x2160@60.00Hz"),
            Value::from("1920x1080@30.00Hz"),
        ];
        for requested in [
            "3840x2160@6",
            "3840x2160@30",
            "3840x2160@600",
            "1920x1080@60",
        ] {
            assert_eq!(
                resolve_mode(requested, &available),
                None,
                "accepted {requested}"
            );
        }
        assert_eq!(resolve_mode("3840x2160@60", &[]), None);
    }

    #[test]
    fn parses_monitor_snapshot() {
        let value = serde_json::json!({
            "name": "HDMI-A-2",
            "width": 3840,
            "height": 2160,
            "refreshRate": 60.0,
            "x": 6000,
            "y": 675,
            "scale": 1.0,
            "transform": 0,
            "activeWorkspace": { "id": 3, "name": "3" }
        });
        let monitor = parse_monitor(&value).unwrap();
        assert_eq!(monitor.name, "HDMI-A-2");
        assert_eq!(monitor.width, 3840);
        assert_eq!(monitor.x, 6000);
        assert_eq!(monitor.active_workspace.as_deref(), Some("3"));
    }

    #[test]
    fn monitor_restore_reenables_output_and_clears_tv_transform() {
        let monitor = MonitorSnapshot {
            name: "DP-3".into(),
            width: 3840,
            height: 2160,
            refresh_rate: 60.0,
            x: -2160,
            y: 1092,
            scale: 1.25,
            transform: 0,
            active_workspace: None,
        };
        assert_eq!(
            restore_monitor_command(&monitor),
            r#"hl.monitor({ output = "DP-3", mode = "3840x2160@60.000", position = "-2160x1092", scale = 1.25, transform = 0, disabled = false })"#
        );
    }

    #[test]
    fn runtime_state_is_scoped_below_xdg_runtime_dir() {
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", temp.path()) };
        assert_eq!(runtime_dir().unwrap(), temp.path().join("tv-mode"));
    }

    #[test]
    fn snapshot_without_vigil_state_remains_recoverable() {
        let snapshot: SessionSnapshot = serde_json::from_value(serde_json::json!({
            "version": 1,
            "hyprland_instance": "test",
            "monitors": [],
            "workspaces": [],
            "focused_monitor": null,
            "focused_workspace": null,
            "default_sink": "test-sink",
            "audio_card_profile": "test-profile",
            "power_profile": null,
            "hypridle_active": false
        }))
        .unwrap();

        assert_eq!(snapshot.vigil_presentation_hidden, None);
        assert_eq!(snapshot.vigil_dnd, None);
    }

    #[test]
    fn unavailable_vigil_state_is_optional() {
        let state = optional_vigil_state::<bool>("read test state", Err(anyhow!("offline")));
        assert_eq!(state, None);
        try_vigil_action("apply test state", Err(anyhow!("offline")));
    }
}
