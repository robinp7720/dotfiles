use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Cursor};
use std::os::unix::net::UnixStream;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::StateUpdate;

use super::{
    CommandEnv, CommandRunner, CompositorAction, CompositorAdapter, NormalizedState, RawWindow,
    StateTracker, reconnect_error,
};

pub struct HyprlandAdapter {
    tracker: StateTracker,
    events: Box<dyn BufRead + Send>,
    pending: VecDeque<StateUpdate>,
    command_runner: CommandRunner,
    command_env: CommandEnv,
    active_keyboard: Option<String>,
}

impl HyprlandAdapter {
    pub fn from_env(signature: &str) -> Result<Self> {
        let snapshot = HyprSnapshotBundle {
            monitors: read_hyprctl_json("monitors")?,
            workspaces: read_hyprctl_json("workspaces")?,
            clients: read_hyprctl_json("clients")?,
            devices: read_hyprctl_json("devices")?,
        };
        let socket_path = std::env::var("XDG_RUNTIME_DIR")
            .map(|runtime| format!("{runtime}/hypr/{signature}/.socket2.sock"))
            .context("XDG_RUNTIME_DIR is not set for Hyprland")?;
        let stream = UnixStream::connect(&socket_path)
            .with_context(|| format!("failed to connect to Hyprland event socket {socket_path}"))?;

        Self::from_sources(
            snapshot,
            Box::new(BufReader::new(stream)),
            Box::new(default_command_runner),
        )
    }

    pub fn new_for_test<F>(snapshot_json: &str, events: &str, command_runner: F) -> Self
    where
        F: FnMut(&str, &[String], &CommandEnv) -> Result<()> + Send + 'static,
    {
        let snapshot: HyprSnapshotBundle =
            serde_json::from_str(snapshot_json).expect("parse Hyprland fixture snapshot");
        Self::from_sources(
            snapshot,
            Box::new(Cursor::new(events.as_bytes().to_vec())),
            Box::new(command_runner),
        )
        .expect("build Hyprland fixture adapter")
    }

    fn from_sources(
        snapshot: HyprSnapshotBundle,
        events: Box<dyn BufRead + Send>,
        command_runner: CommandRunner,
    ) -> Result<Self> {
        let mut tracker = StateTracker::default();
        let active_keyboard = apply_snapshot(&mut tracker.state, snapshot)?;
        Ok(Self {
            tracker,
            events,
            pending: VecDeque::new(),
            command_runner,
            command_env: Vec::new(),
            active_keyboard,
        })
    }
}

impl CompositorAdapter for HyprlandAdapter {
    fn initial_snapshot(&mut self) -> Result<Vec<StateUpdate>> {
        Ok(self.tracker.snapshot_updates())
    }

    fn next_update(&mut self) -> Result<StateUpdate> {
        loop {
            if let Some(update) = self.pending.pop_front() {
                return Ok(update);
            }

            let mut line = String::new();
            let read = self
                .events
                .read_line(&mut line)
                .map_err(|error| reconnect_error(error))?;
            if read == 0 {
                bail!(reconnect_error("Hyprland event stream reached EOF"));
            }

            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }

            apply_event(
                &mut self.tracker.state,
                &mut self.pending,
                &mut self.active_keyboard,
                trimmed,
            )?;
            self.tracker.push_diffs(&mut self.pending);
        }
    }

    fn execute(&mut self, action: CompositorAction) -> Result<()> {
        match action {
            CompositorAction::SwitchWorkspace { output, workspace } => {
                (self.command_runner)(
                    "hyprctl",
                    &["dispatch".to_string(), "focusmonitor".to_string(), output],
                    &self.command_env,
                )?;
                (self.command_runner)(
                    "hyprctl",
                    &["dispatch".to_string(), "workspace".to_string(), workspace],
                    &self.command_env,
                )?;
            }
            CompositorAction::CycleWorkspace { output, direction } => {
                (self.command_runner)(
                    "hyprctl",
                    &["dispatch".to_string(), "focusmonitor".to_string(), output],
                    &self.command_env,
                )?;
                let step = match direction {
                    crate::Direction::Previous => "e-1",
                    crate::Direction::Next => "e+1",
                };
                (self.command_runner)(
                    "hyprctl",
                    &[
                        "dispatch".to_string(),
                        "workspace".to_string(),
                        step.to_string(),
                    ],
                    &self.command_env,
                )?;
            }
            CompositorAction::FocusWindow { window_id, .. } => {
                let window = if window_id.starts_with("address:") {
                    window_id
                } else {
                    format!("address:{window_id}")
                };
                (self.command_runner)(
                    "hyprctl",
                    &["dispatch".to_string(), "focuswindow".to_string(), window],
                    &self.command_env,
                )?;
            }
            CompositorAction::CycleKeyboardLayout => {
                let Some(keyboard) = self.active_keyboard.clone() else {
                    bail!("cannot cycle Hyprland keyboard layout without a known keyboard device");
                };
                (self.command_runner)(
                    "hyprctl",
                    &["switchxkblayout".to_string(), keyboard, "next".to_string()],
                    &self.command_env,
                )?;
            }
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct HyprSnapshotBundle {
    monitors: Vec<HyprMonitor>,
    workspaces: Vec<HyprWorkspace>,
    clients: Vec<HyprClient>,
    devices: HyprDevices,
}

#[derive(Deserialize)]
struct HyprMonitor {
    name: String,
    focused: bool,
    #[serde(rename = "activeWorkspace")]
    active_workspace: HyprWorkspaceRef,
    #[serde(rename = "lastWindow")]
    last_window: Option<String>,
}

#[derive(Deserialize)]
struct HyprWorkspace {
    id: i64,
    name: String,
    monitor: String,
}

#[derive(Deserialize)]
struct HyprWorkspaceRef {
    id: i64,
}

#[derive(Deserialize)]
struct HyprClient {
    address: String,
    workspace: HyprClientWorkspace,
    class: Option<String>,
    title: Option<String>,
    urgent: bool,
    #[serde(default = "default_true")]
    mapped: bool,
}

#[derive(Deserialize)]
struct HyprClientWorkspace {
    id: i64,
}

#[derive(Deserialize)]
struct HyprDevices {
    keyboards: Vec<HyprKeyboard>,
}

#[derive(Deserialize)]
struct HyprKeyboard {
    name: String,
    active_keymap: Option<String>,
}

fn default_true() -> bool {
    true
}

fn apply_snapshot(
    state: &mut NormalizedState,
    snapshot: HyprSnapshotBundle,
) -> Result<Option<String>> {
    let mut workspace_to_output = std::collections::BTreeMap::new();

    for monitor in snapshot.monitors {
        state.ensure_output(&monitor.name);
        if let Some(output) = state.outputs.get_mut(&monitor.name) {
            output.last_window_id = monitor
                .last_window
                .filter(|value| !value.trim().is_empty())
                .map(|value| super::normalize_window_id(&value));
        }
        if monitor.focused {
            state.focused_output = Some(monitor.name.clone());
        }
        workspace_to_output.insert(
            monitor.active_workspace.id.to_string(),
            monitor.name.clone(),
        );
    }

    for workspace in snapshot.workspaces {
        state.ensure_output(&workspace.monitor);
        state.workspaces.insert(
            workspace.id.to_string(),
            super::RawWorkspace {
                id: workspace.id.to_string(),
                label: workspace.name,
                output: workspace.monitor.clone(),
                active: workspace_to_output
                    .get(&workspace.id.to_string())
                    .map(|output| output == &workspace.monitor)
                    .unwrap_or(false),
                urgent_hint: false,
            },
        );
    }

    for client in snapshot.clients.into_iter().filter(|client| client.mapped) {
        let window_id = super::normalize_window_id(&client.address);
        state.windows.insert(
            window_id.clone(),
            RawWindow {
                id: window_id,
                app_id: client.class,
                title: client.title.unwrap_or_default(),
                workspace_id: Some(client.workspace.id.to_string()),
                urgent: client.urgent,
            },
        );
    }

    let keyboard = snapshot.devices.keyboards.into_iter().next();
    if let Some(keyboard) = keyboard.as_ref() {
        state.keyboard_layout = keyboard.active_keymap.clone();
    }
    Ok(keyboard.map(|keyboard| keyboard.name))
}

fn apply_event(
    state: &mut NormalizedState,
    _pending: &mut VecDeque<StateUpdate>,
    active_keyboard: &mut Option<String>,
    line: &str,
) -> Result<()> {
    let Some((event, payload)) = line.split_once(">>") else {
        bail!(reconnect_error(format!(
            "malformed Hyprland event line: {line}"
        )));
    };

    match event {
        "focusedmonv2" => {
            let mut parts = payload.splitn(2, ',');
            let output = parts
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| reconnect_error("focusedmonv2 missing output"))?;
            let workspace_id = parts
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| reconnect_error("focusedmonv2 missing workspace id"))?;
            state.ensure_output(output);
            state.focused_output = Some(output.to_string());
            for workspace in state.workspaces.values_mut() {
                if workspace.output == output {
                    workspace.active = workspace.id == workspace_id;
                }
            }
        }
        "activewindowv2" => {
            let Some(output) = state.focused_output.clone() else {
                return Ok(());
            };
            let Some(raw_output) = state.outputs.get_mut(&output) else {
                return Ok(());
            };
            raw_output.last_window_id = if payload.trim().is_empty() {
                None
            } else {
                Some(super::normalize_window_id(payload))
            };
        }
        "windowtitlev2" => {
            let mut parts = payload.splitn(2, ',');
            let address = parts
                .next()
                .ok_or_else(|| reconnect_error("windowtitlev2 missing window address"))?;
            let title = parts
                .next()
                .ok_or_else(|| reconnect_error("windowtitlev2 missing window title"))?;
            let window_id = super::normalize_window_id(address);
            let window = state
                .windows
                .entry(window_id.clone())
                .or_insert_with(|| RawWindow {
                    id: window_id,
                    ..RawWindow::default()
                });
            window.title = title.to_string();
        }
        "movewindowv2" => {
            let mut parts = payload.splitn(3, ',');
            let address = parts
                .next()
                .ok_or_else(|| reconnect_error("movewindowv2 missing window address"))?;
            let workspace_id = parts
                .next()
                .ok_or_else(|| reconnect_error("movewindowv2 missing workspace id"))?;
            let window_id = super::normalize_window_id(address);
            let window = state
                .windows
                .entry(window_id.clone())
                .or_insert_with(|| RawWindow {
                    id: window_id,
                    ..RawWindow::default()
                });
            window.workspace_id = Some(workspace_id.to_string());
        }
        "urgent" => {
            let window_id = super::normalize_window_id(payload);
            if let Some(window) = state.windows.get_mut(&window_id) {
                window.urgent = true;
            }
        }
        "activelayout" => {
            let mut parts = payload.splitn(2, ',');
            let keyboard = parts
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| reconnect_error("activelayout missing keyboard name"))?;
            let layout = parts
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| reconnect_error("activelayout missing layout name"))?;
            *active_keyboard = Some(keyboard.to_string());
            state.keyboard_layout = Some(layout.to_string());
        }
        "workspacev2" => {
            let mut parts = payload.splitn(2, ',');
            let workspace_id = parts
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| reconnect_error("workspacev2 missing workspace id"))?;
            if let Some(target_output) = state
                .workspaces
                .get(workspace_id)
                .map(|workspace| workspace.output.clone())
            {
                for workspace in state.workspaces.values_mut() {
                    if workspace.output == target_output {
                        workspace.active = workspace.id == workspace_id;
                    }
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn read_hyprctl_json<T>(command: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let output = Command::new("hyprctl")
        .arg("-j")
        .arg(command)
        .output()
        .with_context(|| format!("failed to execute hyprctl -j {command}"))?;
    if !output.status.success() {
        bail!(
            "hyprctl -j {command} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("hyprctl JSON output was not UTF-8")?;
    serde_json::from_str(&stdout)
        .with_context(|| format!("failed to parse hyprctl -j {command} output"))
}

fn default_command_runner(program: &str, args: &[String], env: &CommandEnv) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .envs(
            env.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .status()
        .with_context(|| {
            format!(
                "failed to execute compositor command: {program} {}",
                args.join(" ")
            )
        })?;
    if !status.success() {
        bail!("compositor command failed: {program} {}", args.join(" "));
    }
    Ok(())
}
