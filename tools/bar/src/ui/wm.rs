use crate::{AppConfig, BarSnapshot, OutputState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowGroupSpec {
    pub output_name: String,
    pub workspace_id: Option<String>,
    pub workspace_label: String,
    pub windows: Vec<WindowMenuItemSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowMenuItemSpec {
    pub output_name: String,
    pub window_id: String,
    pub title: String,
    pub app_id: Option<String>,
    pub focused: bool,
}

pub fn select_primary_output(snapshot: &BarSnapshot, config: &AppConfig) -> Option<String> {
    if let Some(primary_output) = config
        .primary_output
        .as_ref()
        .filter(|name| snapshot.outputs.contains_key(name.as_str()))
    {
        return Some(primary_output.clone());
    }

    if let Some(focused_output) = snapshot
        .focused_output
        .as_ref()
        .filter(|name| snapshot.outputs.contains_key(name.as_str()))
    {
        return Some(focused_output.clone());
    }

    snapshot.outputs.keys().next().cloned()
}

pub fn title_for_output(output: &OutputState) -> String {
    if let Some(window) = output.focused_window.as_ref() {
        if !window.title.trim().is_empty() {
            return window.title.clone();
        }
        if let Some(app_id) = window
            .app_id
            .as_ref()
            .filter(|app_id| !app_id.trim().is_empty())
        {
            return app_id.clone();
        }
    }

    output
        .workspaces
        .iter()
        .find(|workspace| workspace.active)
        .or_else(|| output.workspaces.first())
        .map(|workspace| workspace.label.clone())
        .unwrap_or_else(|| output.name.clone())
}

pub fn window_groups(snapshot: &BarSnapshot) -> Vec<WindowGroupSpec> {
    snapshot
        .outputs
        .values()
        .filter_map(|output| {
            output.focused_window.as_ref().map(|window| {
                let workspace = output
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.active)
                    .or_else(|| output.workspaces.first());

                WindowGroupSpec {
                    output_name: output.name.clone(),
                    workspace_id: workspace.map(|workspace| workspace.id.clone()),
                    workspace_label: workspace
                        .map(|workspace| workspace.label.clone())
                        .unwrap_or_else(|| output.name.clone()),
                    windows: vec![WindowMenuItemSpec {
                        output_name: output.name.clone(),
                        window_id: window.id.clone(),
                        title: if window.title.trim().is_empty() {
                            window
                                .app_id
                                .clone()
                                .unwrap_or_else(|| "Window".to_string())
                        } else {
                            window.title.clone()
                        },
                        app_id: window.app_id.clone(),
                        focused: true,
                    }],
                }
            })
        })
        .collect()
}
