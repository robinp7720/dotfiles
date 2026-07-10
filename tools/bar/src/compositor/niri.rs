use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Cursor};
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::StateUpdate;

use super::{
    CommandRunner, CompositorAction, CompositorAdapter, NormalizedState, RawWindow, StateTracker,
    reconnect_error,
};

pub struct NiriAdapter {
    tracker: StateTracker,
    events: Box<dyn BufRead + Send>,
    _event_child: Option<Child>,
    pending: VecDeque<StateUpdate>,
    command_runner: CommandRunner,
    keyboard_layouts: Vec<String>,
}

impl NiriAdapter {
    pub fn from_env(_socket: &str) -> Result<Self> {
        let outputs = read_niri_json(&["msg", "--json", "outputs"])?;
        let workspaces = read_niri_json(&["msg", "--json", "workspaces"])?;
        let windows = read_niri_json(&["msg", "--json", "windows"])?;
        let keyboard_layouts = read_niri_json(&["msg", "--json", "keyboard-layouts"])?;
        let mut child = Command::new("niri")
            .args(["msg", "--json", "event-stream"])
            .stdout(Stdio::piped())
            .spawn()
            .context("failed to start niri event stream")?;
        let stdout = child
            .stdout
            .take()
            .context("niri event stream stdout was unavailable")?;

        Self::from_sources(
            outputs,
            workspaces,
            windows,
            keyboard_layouts,
            Box::new(BufReader::new(stdout)),
            Some(child),
            Box::new(default_command_runner),
        )
    }

    pub fn new_for_test<F>(
        outputs_json: &str,
        workspaces_json: &str,
        windows_json: &str,
        keyboard_layouts_json: &str,
        events: &str,
        command_runner: F,
    ) -> Self
    where
        F: FnMut(&str, &[String]) -> Result<()> + Send + 'static,
    {
        Self::from_sources(
            outputs_json.to_string(),
            workspaces_json.to_string(),
            windows_json.to_string(),
            keyboard_layouts_json.to_string(),
            Box::new(Cursor::new(events.as_bytes().to_vec())),
            None,
            Box::new(command_runner),
        )
        .expect("build Niri fixture adapter")
    }

    fn from_sources(
        outputs_json: String,
        workspaces_json: String,
        windows_json: String,
        keyboard_layouts_json: String,
        events: Box<dyn BufRead + Send>,
        event_child: Option<Child>,
        command_runner: CommandRunner,
    ) -> Result<Self> {
        let mut tracker = StateTracker::default();
        let keyboard_layouts = apply_snapshot(
            &mut tracker.state,
            &outputs_json,
            &workspaces_json,
            &windows_json,
            &keyboard_layouts_json,
        )?;

        Ok(Self {
            tracker,
            events,
            _event_child: event_child,
            pending: VecDeque::new(),
            command_runner,
            keyboard_layouts,
        })
    }
}

impl CompositorAdapter for NiriAdapter {
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
                bail!(reconnect_error("Niri event stream reached EOF"));
            }

            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }

            apply_event(
                &mut self.tracker.state,
                &mut self.pending,
                &mut self.keyboard_layouts,
                trimmed,
            )?;
            self.tracker.push_diffs(&mut self.pending);
        }
    }

    fn execute(&mut self, action: CompositorAction) -> Result<()> {
        match action {
            CompositorAction::SwitchWorkspace { output, workspace } => {
                (self.command_runner)(
                    "niri",
                    &[
                        "msg".to_string(),
                        "action".to_string(),
                        "focus-monitor".to_string(),
                        output,
                    ],
                )?;
                (self.command_runner)(
                    "niri",
                    &[
                        "msg".to_string(),
                        "action".to_string(),
                        "focus-workspace".to_string(),
                        workspace,
                    ],
                )?;
            }
            CompositorAction::CycleWorkspace { output, direction } => {
                (self.command_runner)(
                    "niri",
                    &[
                        "msg".to_string(),
                        "action".to_string(),
                        "focus-monitor".to_string(),
                        output,
                    ],
                )?;
                let action = match direction {
                    crate::Direction::Previous => "focus-workspace-up",
                    crate::Direction::Next => "focus-workspace-down",
                };
                (self.command_runner)(
                    "niri",
                    &["msg".to_string(), "action".to_string(), action.to_string()],
                )?;
            }
            CompositorAction::FocusWindow { window_id, .. } => {
                (self.command_runner)(
                    "niri",
                    &[
                        "msg".to_string(),
                        "action".to_string(),
                        "focus-window".to_string(),
                        "--id".to_string(),
                        window_id,
                    ],
                )?;
            }
            CompositorAction::CycleKeyboardLayout => {
                (self.command_runner)(
                    "niri",
                    &[
                        "msg".to_string(),
                        "action".to_string(),
                        "switch-layout".to_string(),
                        "next".to_string(),
                    ],
                )?;
            }
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct NiriOutput {
    name: String,
    #[serde(default)]
    is_focused: bool,
}

#[derive(Deserialize)]
struct NiriWorkspace {
    id: u64,
    idx: u8,
    name: Option<String>,
    output: Option<String>,
    is_urgent: bool,
    is_active: bool,
    is_focused: bool,
    active_window_id: Option<u64>,
}

#[derive(Deserialize)]
struct NiriWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    workspace_id: Option<u64>,
    is_focused: bool,
    is_urgent: bool,
}

#[derive(Deserialize)]
struct NiriKeyboardLayouts {
    names: Vec<String>,
    current_idx: usize,
}

fn apply_snapshot(
    state: &mut NormalizedState,
    outputs_json: &str,
    workspaces_json: &str,
    windows_json: &str,
    keyboard_layouts_json: &str,
) -> Result<Vec<String>> {
    let outputs: Vec<NiriOutput> =
        serde_json::from_str(outputs_json).context("failed to parse niri outputs JSON")?;
    let workspaces: Vec<NiriWorkspace> =
        serde_json::from_str(workspaces_json).context("failed to parse niri workspaces JSON")?;
    let windows: Vec<NiriWindow> =
        serde_json::from_str(windows_json).context("failed to parse niri windows JSON")?;
    let keyboard_layouts: NiriKeyboardLayouts = serde_json::from_str(keyboard_layouts_json)
        .context("failed to parse niri keyboard layouts JSON")?;

    for output in outputs {
        state.ensure_output(&output.name);
        if output.is_focused {
            state.focused_output = Some(output.name);
        }
    }

    for workspace in workspaces {
        let Some(output_name) = workspace.output else {
            continue;
        };
        state.ensure_output(&output_name);
        let workspace_id = workspace.id.to_string();
        state.workspaces.insert(
            workspace_id.clone(),
            super::RawWorkspace {
                id: workspace_id.clone(),
                label: workspace.name.unwrap_or_else(|| workspace.idx.to_string()),
                output: output_name.clone(),
                active: workspace.is_active,
                urgent_hint: workspace.is_urgent,
            },
        );
        if workspace.is_focused {
            state.focused_output = Some(output_name.clone());
        }
        if workspace.is_active {
            if let Some(output) = state.outputs.get_mut(&output_name) {
                output.last_window_id = workspace.active_window_id.map(|id| id.to_string());
            }
        }
    }

    for window in windows {
        state.windows.insert(
            window.id.to_string(),
            RawWindow {
                id: window.id.to_string(),
                app_id: window.app_id,
                title: window.title.unwrap_or_default(),
                workspace_id: window.workspace_id.map(|id| id.to_string()),
                urgent: window.is_urgent,
            },
        );
        if window.is_focused {
            if let Some(workspace_id) = window.workspace_id.map(|id| id.to_string()) {
                if let Some(workspace) = state.workspaces.get(&workspace_id) {
                    state.focused_output = Some(workspace.output.clone());
                    if let Some(output) = state.outputs.get_mut(&workspace.output) {
                        output.last_window_id = Some(window.id.to_string());
                    }
                }
            }
        }
    }

    if keyboard_layouts.current_idx < keyboard_layouts.names.len() {
        state.keyboard_layout = Some(keyboard_layouts.names[keyboard_layouts.current_idx].clone());
    }

    Ok(keyboard_layouts.names)
}

fn apply_event(
    state: &mut NormalizedState,
    _pending: &mut VecDeque<StateUpdate>,
    keyboard_layouts: &mut Vec<String>,
    line: &str,
) -> Result<()> {
    let value: Value =
        serde_json::from_str(line).map_err(|error| reconnect_error(format!("{error}: {line}")))?;

    if let Some(payload) = value.get("WorkspaceActivated") {
        let workspace_id = payload
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| reconnect_error("WorkspaceActivated missing id"))?;
        let focused = payload
            .get("focused")
            .and_then(Value::as_bool)
            .ok_or_else(|| reconnect_error("WorkspaceActivated missing focused flag"))?;
        if let Some(workspace) = state.workspaces.get(&workspace_id.to_string()).cloned() {
            for candidate in state.workspaces.values_mut() {
                if candidate.output == workspace.output {
                    candidate.active = candidate.id == workspace.id;
                }
            }
            if focused {
                state.focused_output = Some(workspace.output);
            }
        }
        return Ok(());
    }

    if let Some(payload) = value.get("WindowOpenedOrChanged") {
        let window: NiriWindowPayload = serde_json::from_value(
            payload
                .get("window")
                .cloned()
                .ok_or_else(|| reconnect_error("WindowOpenedOrChanged missing window"))?,
        )
        .map_err(|error| reconnect_error(error))?;
        let window_id = window.id.to_string();
        state.windows.insert(
            window_id.clone(),
            RawWindow {
                id: window_id.clone(),
                app_id: window.app_id,
                title: window.title.unwrap_or_default(),
                workspace_id: window.workspace_id.map(|id| id.to_string()),
                urgent: window.is_urgent,
            },
        );
        if window.is_focused {
            if let Some(workspace_id) = window.workspace_id.map(|id| id.to_string()) {
                if let Some(workspace) = state.workspaces.get(&workspace_id) {
                    state.focused_output = Some(workspace.output.clone());
                    if let Some(output) = state.outputs.get_mut(&workspace.output) {
                        output.last_window_id = Some(window_id);
                    }
                }
            }
        }
        return Ok(());
    }

    if let Some(payload) = value.get("WindowUrgencyChanged") {
        let window_id = payload
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| reconnect_error("WindowUrgencyChanged missing id"))?;
        let urgent = payload
            .get("urgent")
            .and_then(Value::as_bool)
            .ok_or_else(|| reconnect_error("WindowUrgencyChanged missing urgent flag"))?;
        if let Some(window) = state.windows.get_mut(&window_id.to_string()) {
            window.urgent = urgent;
        }
        return Ok(());
    }

    if let Some(payload) = value.get("KeyboardLayoutSwitched") {
        let idx = payload
            .get("idx")
            .and_then(Value::as_u64)
            .ok_or_else(|| reconnect_error("KeyboardLayoutSwitched missing idx"))?
            as usize;
        let Some(layout) = keyboard_layouts.get(idx) else {
            bail!(reconnect_error(format!(
                "KeyboardLayoutSwitched index {idx} out of range"
            )));
        };
        state.keyboard_layout = Some(layout.clone());
        return Ok(());
    }

    if let Some(payload) = value.get("KeyboardLayoutsChanged") {
        let parsed: NiriKeyboardLayouts =
            serde_json::from_value(payload.get("keyboard_layouts").cloned().ok_or_else(|| {
                reconnect_error("KeyboardLayoutsChanged missing keyboard_layouts")
            })?)
            .map_err(|error| reconnect_error(error))?;
        *keyboard_layouts = parsed.names.clone();
        if parsed.current_idx < keyboard_layouts.len() {
            state.keyboard_layout = Some(keyboard_layouts[parsed.current_idx].clone());
        }
    }

    Ok(())
}

#[derive(Deserialize)]
struct NiriWindowPayload {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    workspace_id: Option<u64>,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_urgent: bool,
}

fn read_niri_json(args: &[&str]) -> Result<String> {
    let output = Command::new("niri")
        .args(args)
        .output()
        .with_context(|| format!("failed to execute niri {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "niri {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("niri JSON output was not UTF-8")
}

fn default_command_runner(program: &str, args: &[String]) -> Result<()> {
    let status = Command::new(program).args(args).status().with_context(|| {
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
