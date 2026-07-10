use std::collections::{BTreeMap, VecDeque};

use anyhow::{Result, anyhow, bail};

use crate::{Direction, OutputState, StateUpdate, SystemUpdate, WindowState, WorkspaceState};

pub mod hyprland;
pub mod niri;

pub use hyprland::HyprlandAdapter;
pub use niri::NiriAdapter;

pub trait CompositorAdapter: Send {
    fn initial_snapshot(&mut self) -> Result<Vec<StateUpdate>>;
    fn next_update(&mut self) -> Result<StateUpdate>;
    fn execute(&mut self, action: CompositorAction) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositorAction {
    SwitchWorkspace {
        output: String,
        workspace: String,
    },
    CycleWorkspace {
        output: String,
        direction: Direction,
    },
    FocusWindow {
        output: String,
        window_id: String,
    },
    CycleKeyboardLayout,
}

pub fn detect_compositor(env: &[(&str, &str)]) -> Result<Box<dyn CompositorAdapter>> {
    let niri_socket = env
        .iter()
        .find_map(|(key, value)| (*key == "NIRI_SOCKET" && !value.is_empty()).then_some(*value));
    if let Some(socket) = niri_socket {
        return NiriAdapter::from_env(socket).map(|adapter| Box::new(adapter) as Box<_>);
    }

    let hyprland_signature = env.iter().find_map(|(key, value)| {
        (*key == "HYPRLAND_INSTANCE_SIGNATURE" && !value.is_empty()).then_some(*value)
    });
    if let Some(signature) = hyprland_signature {
        return HyprlandAdapter::from_env(signature).map(|adapter| Box::new(adapter) as Box<_>);
    }

    bail!(
        "unsupported session: expected NIRI_SOCKET or HYPRLAND_INSTANCE_SIGNATURE in the environment"
    );
}

pub(crate) type CommandEnv = Vec<(String, String)>;
pub(crate) type CommandRunner = Box<dyn FnMut(&str, &[String], &CommandEnv) -> Result<()> + Send>;

#[derive(Clone, Debug, Default)]
pub(crate) struct NormalizedState {
    pub outputs: BTreeMap<String, RawOutput>,
    pub workspaces: BTreeMap<String, RawWorkspace>,
    pub windows: BTreeMap<String, RawWindow>,
    pub focused_output: Option<String>,
    pub keyboard_layout: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StateTracker {
    pub state: NormalizedState,
    outputs_cache: Vec<OutputState>,
    focused_output_cache: Option<String>,
    keyboard_layout_cache: Option<String>,
}

impl StateTracker {
    pub(crate) fn snapshot_updates(&mut self) -> Vec<StateUpdate> {
        let mut pending = VecDeque::new();
        self.push_diffs(&mut pending);
        pending.into_iter().collect()
    }

    pub(crate) fn push_diffs(&mut self, pending: &mut VecDeque<StateUpdate>) {
        let outputs = self.state.render_outputs();
        if outputs != self.outputs_cache {
            self.outputs_cache = outputs.clone();
            pending.push_back(StateUpdate::Outputs(outputs));
        }

        if self.state.focused_output != self.focused_output_cache {
            self.focused_output_cache = self.state.focused_output.clone();
            pending.push_back(StateUpdate::FocusedOutput(
                self.focused_output_cache.clone(),
            ));
        }

        if self.state.keyboard_layout != self.keyboard_layout_cache {
            self.keyboard_layout_cache = self.state.keyboard_layout.clone();
            pending.push_back(StateUpdate::System(SystemUpdate::KeyboardLayout(
                self.keyboard_layout_cache.clone(),
            )));
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawOutput {
    pub name: String,
    pub last_window_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawWorkspace {
    pub id: String,
    pub label: String,
    pub output: String,
    pub active: bool,
    pub urgent_hint: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawWindow {
    pub id: String,
    pub app_id: Option<String>,
    pub title: String,
    pub workspace_id: Option<String>,
    pub urgent: bool,
}

impl NormalizedState {
    pub(crate) fn ensure_output(&mut self, output: &str) {
        self.outputs
            .entry(output.to_string())
            .or_insert_with(|| RawOutput {
                name: output.to_string(),
                ..RawOutput::default()
            });
    }

    pub(crate) fn render_outputs(&self) -> Vec<OutputState> {
        self.outputs
            .values()
            .map(|output| {
                let mut workspaces = self
                    .workspaces
                    .values()
                    .filter(|workspace| workspace.output == output.name)
                    .map(|workspace| WorkspaceState {
                        id: workspace.id.clone(),
                        label: workspace.label.clone(),
                        output: workspace.output.clone(),
                        active: workspace.active,
                        urgent: self.workspace_is_urgent(&workspace.id),
                        changed_at: 0,
                    })
                    .collect::<Vec<_>>();
                workspaces.sort_by(workspace_sort_key);

                let focused_window = output.last_window_id.as_ref().and_then(|window_id| {
                    self.windows.get(window_id).map(|window| WindowState {
                        id: window.id.clone(),
                        app_id: window.app_id.clone(),
                        title: window.title.clone(),
                        urgent: window.urgent,
                        changed_at: 0,
                    })
                });

                let urgent = workspaces.iter().any(|workspace| workspace.urgent)
                    || self
                        .windows
                        .values()
                        .filter(|window| {
                            window
                                .workspace_id
                                .as_ref()
                                .and_then(|workspace_id| self.workspaces.get(workspace_id))
                                .map(|workspace| workspace.output == output.name)
                                .unwrap_or(false)
                        })
                        .any(|window| window.urgent);

                OutputState {
                    name: output.name.clone(),
                    workspaces,
                    focused_window,
                    urgent,
                    changed_at: 0,
                }
            })
            .collect()
    }

    fn workspace_is_urgent(&self, workspace_id: &str) -> bool {
        self.workspaces
            .get(workspace_id)
            .map(|workspace| workspace.urgent_hint)
            .unwrap_or(false)
            || self
                .windows
                .values()
                .any(|window| window.workspace_id.as_deref() == Some(workspace_id) && window.urgent)
    }
}

fn workspace_sort_key(workspace: &WorkspaceState, other: &WorkspaceState) -> std::cmp::Ordering {
    match (
        workspace.id.parse::<i64>().ok(),
        other.id.parse::<i64>().ok(),
    ) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => workspace.label.cmp(&other.label),
    }
}

pub(crate) fn reconnect_error(detail: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("compositor stream requires resync: {detail}")
}

pub(crate) fn normalize_window_id(value: &str) -> String {
    let trimmed = value.trim();
    if let Some(hex) = trimmed.strip_prefix("0x") {
        return u64::from_str_radix(hex, 16)
            .map(|parsed| parsed.to_string())
            .unwrap_or_else(|_| trimmed.to_string());
    }
    trimmed.to_string()
}
