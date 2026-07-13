use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gtk::gdk;
use gtk::gio::prelude::ListModelExtManual;
use gtk::pango::EllipsizeMode;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::{
    ActionIntent, ActionRequest, AppConfig, BarSnapshot, ContextCard, ContextTier, Dismissals,
    OutputRole, WorkspaceState, select_context,
};

use super::context_card::{context_text, context_tier, warning_text};
use super::wm::{WindowGroupSpec, select_primary_output, title_for_output, window_groups};

const BAR_HEIGHT: i32 = 44;
const SURFACE_MARGIN: i32 = 5;
const WORKSPACE_BUTTON_MIN_WIDTH: i32 = 32;
const WORKSPACE_BUTTON_MIN_HEIGHT: i32 = 28;
const CENTER_SLOT_MAX_WIDTH: i32 = 560;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceSpec {
    pub output_name: String,
    pub role: OutputRole,
    pub workspaces: Vec<WorkspaceButtonSpec>,
    pub title: TitleSpec,
    pub context: Option<ContextSpec>,
    pub warning: Option<WarningSpec>,
    pub clock_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceButtonSpec {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub urgent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleSpec {
    pub text: String,
    pub window_groups: Vec<WindowGroupSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextSpec {
    pub card: ContextCard,
    pub text: String,
    pub tier: ContextTier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarningSpec {
    pub text: String,
    pub tier: ContextTier,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderPlan {
    pub workspaces: bool,
    pub title: bool,
    pub context: bool,
    pub warning: bool,
    pub clock: bool,
}

impl RenderPlan {
    pub fn between(previous: Option<&SurfaceSpec>, next: &SurfaceSpec) -> Self {
        match previous {
            None => Self {
                workspaces: true,
                title: true,
                context: true,
                warning: true,
                clock: true,
            },
            Some(previous) => Self {
                workspaces: previous.workspaces != next.workspaces,
                title: previous.title != next.title,
                context: previous.context != next.context,
                warning: previous.warning != next.warning,
                clock: previous.clock_label != next.clock_label,
            },
        }
    }
}

pub fn surface_specs(snapshot: &BarSnapshot, config: &AppConfig) -> Vec<SurfaceSpec> {
    let Some(primary_output) = select_primary_output(snapshot, config) else {
        return Vec::new();
    };

    let groups = window_groups(snapshot);
    let now_epoch = snapshot.system.clock.epoch_seconds;
    let selected_context = select_context(
        snapshot,
        now_epoch,
        &config.thresholds,
        &Dismissals::default(),
    );
    let primary_context = selected_context.as_ref().map(|card| ContextSpec {
        card: card.clone(),
        text: context_text(card),
        tier: context_tier(card),
    });
    let reduced_warning = selected_context
        .as_ref()
        .filter(|card| context_tier(card) == ContextTier::Critical)
        .map(|card| WarningSpec {
            text: warning_text(card),
            tier: ContextTier::Critical,
        });

    let mut specs = Vec::with_capacity(snapshot.outputs.len());

    if let Some(output) = snapshot.outputs.get(&primary_output) {
        specs.push(spec_for_output(
            output,
            OutputRole::Primary,
            snapshot.system.clock.label.clone(),
            groups.clone(),
            primary_context.clone(),
            None,
        ));
    }

    for (output_name, output) in &snapshot.outputs {
        if *output_name == primary_output {
            continue;
        }

        specs.push(spec_for_output(
            output,
            OutputRole::Reduced,
            snapshot.system.clock.label.clone(),
            groups.clone(),
            None,
            reduced_warning.clone(),
        ));
    }

    specs
}

fn spec_for_output(
    output: &crate::OutputState,
    role: OutputRole,
    clock_label: String,
    window_groups: Vec<WindowGroupSpec>,
    context: Option<ContextSpec>,
    warning: Option<WarningSpec>,
) -> SurfaceSpec {
    SurfaceSpec {
        output_name: output.name.clone(),
        role,
        workspaces: output
            .workspaces
            .iter()
            .map(workspace_button_spec)
            .collect(),
        title: TitleSpec {
            text: title_for_output(output),
            window_groups,
        },
        context,
        warning,
        clock_label,
    }
}

fn workspace_button_spec(workspace: &WorkspaceState) -> WorkspaceButtonSpec {
    WorkspaceButtonSpec {
        id: workspace.id.clone(),
        label: workspace.label.clone(),
        active: workspace.active,
        urgent: workspace.urgent,
    }
}

pub struct SurfaceRegistry {
    surfaces: BTreeMap<String, SurfaceHandle>,
}

impl Default for SurfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceRegistry {
    pub fn new() -> Self {
        Self {
            surfaces: BTreeMap::new(),
        }
    }

    pub fn reconcile(
        &mut self,
        application: &gtk::Application,
        snapshot: &BarSnapshot,
        config: &AppConfig,
        action_sender: &Sender<ActionRequest>,
    ) {
        let desired_specs = surface_specs(snapshot, config);
        let Some(display) = gdk::Display::default() else {
            return;
        };

        let monitors = monitors_by_connector(&display);
        let desired_connectors = desired_specs
            .iter()
            .filter(|spec| monitors.contains_key(&spec.output_name))
            .map(|spec| spec.output_name.clone())
            .collect::<BTreeSet<_>>();

        let stale = self
            .surfaces
            .keys()
            .filter(|connector| !desired_connectors.contains(*connector))
            .cloned()
            .collect::<Vec<_>>();
        for connector in stale {
            if let Some(surface) = self.surfaces.remove(&connector) {
                surface.close();
            }
        }

        for spec in desired_specs {
            let Some(monitor) = monitors.get(&spec.output_name) else {
                continue;
            };

            let recreate = self
                .surfaces
                .get(&spec.output_name)
                .is_some_and(|surface| surface.role() != spec.role);

            if recreate {
                if let Some(surface) = self.surfaces.remove(&spec.output_name) {
                    surface.close();
                }
            }

            match self.surfaces.get_mut(&spec.output_name) {
                Some(surface) => surface.render(&spec),
                None => {
                    let surface =
                        SurfaceHandle::new(application, monitor, &spec, action_sender.clone());
                    self.surfaces.insert(spec.output_name.clone(), surface);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        for (_, surface) in std::mem::take(&mut self.surfaces) {
            surface.close();
        }
    }
}

fn monitors_by_connector(display: &gdk::Display) -> BTreeMap<String, gdk::Monitor> {
    display
        .monitors()
        .snapshot()
        .into_iter()
        .filter_map(|object| object.downcast::<gdk::Monitor>().ok())
        .filter_map(|monitor| {
            monitor
                .connector()
                .map(|connector| (connector.to_string(), monitor))
        })
        .collect()
}

enum SurfaceHandle {
    Primary(PrimarySurface),
    Reduced(ReducedSurface),
}

impl SurfaceHandle {
    fn new(
        application: &gtk::Application,
        monitor: &gdk::Monitor,
        spec: &SurfaceSpec,
        action_sender: Sender<ActionRequest>,
    ) -> Self {
        match spec.role {
            OutputRole::Primary => Self::Primary(PrimarySurface::new(
                application,
                monitor,
                spec,
                action_sender,
            )),
            OutputRole::Reduced => Self::Reduced(ReducedSurface::new(
                application,
                monitor,
                spec,
                action_sender,
            )),
        }
    }

    fn role(&self) -> OutputRole {
        match self {
            Self::Primary(_) => OutputRole::Primary,
            Self::Reduced(_) => OutputRole::Reduced,
        }
    }

    fn render(&mut self, spec: &SurfaceSpec) {
        match self {
            Self::Primary(surface) => surface.render(spec),
            Self::Reduced(surface) => surface.render(spec),
        }
    }

    fn close(self) {
        match self {
            Self::Primary(surface) => surface.window.close(),
            Self::Reduced(surface) => surface.window.close(),
        }
    }
}

pub struct PrimarySurface {
    window: gtk::ApplicationWindow,
    workspaces: gtk::Box,
    title_label: gtk::Label,
    title_groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    context_stack: gtk::Stack,
    context_label: gtk::Label,
    clock_label: gtk::Label,
    action_sender: Sender<ActionRequest>,
    current_spec: Option<SurfaceSpec>,
}

impl PrimarySurface {
    pub fn new(
        application: &gtk::Application,
        monitor: &gdk::Monitor,
        spec: &SurfaceSpec,
        action_sender: Sender<ActionRequest>,
    ) -> Self {
        let window = base_window(application, monitor);

        let grid = gtk::Grid::new();
        grid.set_column_spacing(12);
        grid.set_margin_start(12);
        grid.set_margin_end(12);
        grid.set_size_request(-1, BAR_HEIGHT);

        let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let center_slot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let center_slot_frame = gtk::ScrolledWindow::new();
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        left.set_halign(gtk::Align::Start);
        center_slot.set_hexpand(true);
        center_slot.set_halign(gtk::Align::Center);
        center_slot_frame.set_hexpand(true);
        center_slot_frame.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
        center_slot_frame.set_max_content_width(CENTER_SLOT_MAX_WIDTH);
        center_slot_frame.set_propagate_natural_width(true);
        center_slot_frame.set_child(Some(&center_slot));
        right.set_halign(gtk::Align::End);

        let workspaces = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        left.append(&workspaces);

        let title_button = gtk::Button::new();
        title_button.set_has_frame(false);
        title_button.set_hexpand(true);
        title_button.set_halign(gtk::Align::Fill);
        let title_label = gtk::Label::new(None);
        title_label.set_hexpand(true);
        title_label.set_max_width_chars(48);
        title_label.set_ellipsize(EllipsizeMode::End);
        title_label.set_xalign(0.0);
        title_button.set_child(Some(&title_label));

        let title_groups = Rc::new(RefCell::new(Vec::new()));
        install_title_interactions(
            &title_button,
            title_groups.clone(),
            action_sender.clone(),
            spec.output_name.clone(),
        );

        let context_stack = gtk::Stack::new();
        context_stack.set_halign(gtk::Align::Start);
        context_stack.set_transition_duration(180);
        context_stack.set_transition_type(context_transition_type());
        let context_empty = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let context_label = gtk::Label::new(None);
        context_label.set_xalign(0.0);
        context_label.set_ellipsize(EllipsizeMode::End);
        context_stack.add_named(&context_empty, Some("empty"));
        context_stack.add_named(&context_label, Some("card"));
        context_stack.set_visible_child_name("empty");

        center_slot.append(&title_button);
        center_slot.append(&context_stack);

        let clock_label = gtk::Label::new(None);
        right.append(&clock_label);

        grid.attach(&left, 0, 0, 1, 1);
        grid.attach(&center_slot_frame, 1, 0, 1, 1);
        grid.attach(&right, 2, 0, 1, 1);

        window.set_child(Some(&grid));
        window.present();

        let mut surface = Self {
            window,
            workspaces,
            title_label,
            title_groups,
            context_stack,
            context_label,
            clock_label,
            action_sender,
            current_spec: None,
        };
        surface.render(spec);
        surface
    }

    pub fn render(&mut self, spec: &SurfaceSpec) {
        let plan = RenderPlan::between(self.current_spec.as_ref(), spec);
        if plan == RenderPlan::default() {
            return;
        }

        if plan.workspaces {
            render_workspaces(
                &self.workspaces,
                &self.action_sender,
                &spec.output_name,
                &spec.workspaces,
            );
        }
        if plan.title {
            self.title_label.set_label(&spec.title.text);
            *self.title_groups.borrow_mut() = spec.title.window_groups.clone();
        }
        if plan.clock {
            self.clock_label.set_label(&spec.clock_label);
        }

        if plan.context {
            if let Some(context) = spec.context.as_ref() {
                self.context_label.set_label(&context.text);
                self.context_stack.set_visible_child_name("card");
            } else {
                self.context_label.set_label("");
                self.context_stack.set_visible_child_name("empty");
            }
        }

        self.current_spec = Some(spec.clone());
    }
}

pub struct ReducedSurface {
    window: gtk::ApplicationWindow,
    workspaces: gtk::Box,
    title_label: gtk::Label,
    title_groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    warning_label: gtk::Label,
    clock_label: gtk::Label,
    action_sender: Sender<ActionRequest>,
    current_spec: Option<SurfaceSpec>,
}

impl ReducedSurface {
    pub fn new(
        application: &gtk::Application,
        monitor: &gdk::Monitor,
        spec: &SurfaceSpec,
        action_sender: Sender<ActionRequest>,
    ) -> Self {
        let window = base_window(application, monitor);

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, BAR_HEIGHT);

        let workspaces = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let title_button = gtk::Button::new();
        title_button.set_has_frame(false);
        title_button.set_hexpand(true);
        title_button.set_halign(gtk::Align::Fill);
        let title_label = gtk::Label::new(None);
        title_label.set_hexpand(true);
        title_label.set_max_width_chars(42);
        title_label.set_ellipsize(EllipsizeMode::End);
        title_label.set_xalign(0.0);
        title_button.set_child(Some(&title_label));

        let title_groups = Rc::new(RefCell::new(Vec::new()));
        install_title_interactions(
            &title_button,
            title_groups.clone(),
            action_sender.clone(),
            spec.output_name.clone(),
        );

        let warning_label = gtk::Label::new(None);
        warning_label.set_visible(false);
        warning_label.set_ellipsize(EllipsizeMode::End);

        let clock_label = gtk::Label::new(None);

        root.append(&workspaces);
        root.append(&title_button);
        root.append(&warning_label);
        root.append(&clock_label);

        window.set_child(Some(&root));
        window.present();

        let mut surface = Self {
            window,
            workspaces,
            title_label,
            title_groups,
            warning_label,
            clock_label,
            action_sender,
            current_spec: None,
        };
        surface.render(spec);
        surface
    }

    pub fn render(&mut self, spec: &SurfaceSpec) {
        let plan = RenderPlan::between(self.current_spec.as_ref(), spec);
        if plan == RenderPlan::default() {
            return;
        }

        if plan.workspaces {
            render_workspaces(
                &self.workspaces,
                &self.action_sender,
                &spec.output_name,
                &spec.workspaces,
            );
        }
        if plan.title {
            self.title_label.set_label(&spec.title.text);
            *self.title_groups.borrow_mut() = spec.title.window_groups.clone();
        }
        if plan.clock {
            self.clock_label.set_label(&spec.clock_label);
        }

        if plan.warning {
            if let Some(warning) = spec.warning.as_ref() {
                self.warning_label.set_label(&warning.text);
                self.warning_label.set_visible(true);
            } else {
                self.warning_label.set_label("");
                self.warning_label.set_visible(false);
            }
        }

        self.current_spec = Some(spec.clone());
    }
}

fn base_window(application: &gtk::Application, monitor: &gdk::Monitor) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("cockpit-bar")
        .build();
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(1, BAR_HEIGHT);
    window.init_layer_shell();
    window.set_namespace(Some("cockpit-bar"));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_exclusive_zone(BAR_HEIGHT);
    window.set_margin(Edge::Top, SURFACE_MARGIN);
    window.set_margin(Edge::Left, SURFACE_MARGIN);
    window.set_margin(Edge::Right, SURFACE_MARGIN);
    window.set_monitor(Some(monitor));
    window
}

fn render_workspaces(
    container: &gtk::Box,
    action_sender: &Sender<ActionRequest>,
    output_name: &str,
    workspaces: &[WorkspaceButtonSpec],
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for workspace in workspaces {
        let button = gtk::Button::with_label(&workspace.label);
        button.set_size_request(WORKSPACE_BUTTON_MIN_WIDTH, WORKSPACE_BUTTON_MIN_HEIGHT);
        button.set_has_frame(workspace.active || workspace.urgent);
        let sender = action_sender.clone();
        let output = output_name.to_string();
        let workspace_id = workspace.id.clone();
        button.connect_clicked(move |_| {
            let _ = sender.send(ActionRequest {
                origin: format!("workspace:{output}:{workspace_id}"),
                intent: ActionIntent::SwitchWorkspace {
                    output: output.clone(),
                    workspace: workspace_id.clone(),
                },
            });
        });
        container.append(&button);
    }
}

fn install_title_interactions(
    button: &gtk::Button,
    groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    action_sender: Sender<ActionRequest>,
    output_name: String,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_parent(button);

    let groups_for_click = groups.clone();
    let sender_for_click = action_sender.clone();
    let popover_output = output_name.clone();
    button.connect_clicked(move |button| {
        rebuild_window_popover(
            &popover,
            &groups_for_click.borrow(),
            &sender_for_click,
            &popover_output,
        );
        popover.set_parent(button);
        popover.popup();
    });

    let secondary = gtk::GestureClick::new();
    secondary.set_button(3);
    let secondary_output = output_name.clone();
    secondary.connect_pressed(move |_, _, _, _| {
        let _ = action_sender.send(ActionRequest {
            origin: format!("title-secondary:{secondary_output}"),
            intent: ActionIntent::OpenWindowSearch,
        });
    });
    button.add_controller(secondary);
}

fn rebuild_window_popover(
    popover: &gtk::Popover,
    groups: &[WindowGroupSpec],
    action_sender: &Sender<ActionRequest>,
    origin_output: &str,
) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_top(8);
    column.set_margin_bottom(8);
    column.set_margin_start(8);
    column.set_margin_end(8);

    for group in groups {
        let heading = gtk::Label::new(Some(&format!(
            "{} / {}",
            group.output_name, group.workspace_label
        )));
        heading.set_xalign(0.0);
        column.append(&heading);

        for item in &group.windows {
            let button = gtk::Button::with_label(&item.title);
            button.set_has_frame(false);
            button.set_halign(gtk::Align::Fill);
            let sender = action_sender.clone();
            let output = item.output_name.clone();
            let window_id = item.window_id.clone();
            let origin = format!("window-popover:{origin_output}:{window_id}");
            let popover = popover.clone();
            button.connect_clicked(move |_| {
                let _ = sender.send(ActionRequest {
                    origin: origin.clone(),
                    intent: ActionIntent::FocusWindow {
                        output: output.clone(),
                        window_id: window_id.clone(),
                    },
                });
                popover.popdown();
            });
            column.append(&button);
        }
    }

    popover.set_child(Some(&column));
}

fn context_transition_type() -> gtk::StackTransitionType {
    if gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()) {
        gtk::StackTransitionType::Crossfade
    } else {
        gtk::StackTransitionType::None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AppConfig, BarSnapshot, ClockState, OutputRole, OutputState, PowerProfile, PowerState,
        WindowState, WorkspaceState,
    };

    use super::{RenderPlan, SurfaceSpec, surface_specs};

    #[test]
    fn surface_specs_choose_configured_primary_and_reduce_other_outputs() {
        let snapshot = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("2", "2", "DP-5", true, false)],
                Some(window("editor", "Editor")),
                false,
            ),
            output(
                "HDMI-A-2",
                &[workspace("3", "3", "HDMI-A-2", true, false)],
                Some(window("chat", "Chat")),
                false,
            ),
        ]);

        let specs = surface_specs(&snapshot, &AppConfig::default());

        assert_eq!(
            specs
                .iter()
                .map(|spec| (&spec.output_name, spec.role))
                .collect::<Vec<_>>(),
            vec![
                (&"DP-5".to_string(), OutputRole::Primary),
                (&"DP-4".to_string(), OutputRole::Reduced),
                (&"HDMI-A-2".to_string(), OutputRole::Reduced),
            ]
        );
    }

    #[test]
    fn surface_specs_fall_back_to_focused_output_when_primary_is_missing() {
        let snapshot = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "HDMI-A-2",
                &[workspace("3", "3", "HDMI-A-2", true, false)],
                Some(window("chat", "Chat")),
                false,
            ),
        ]);

        let specs = surface_specs(&snapshot, &AppConfig::default());

        assert_eq!(specs[0].output_name, "DP-4");
        assert_eq!(specs[0].role, OutputRole::Primary);
    }

    #[test]
    fn surface_specs_restore_configured_primary_when_it_returns() {
        let without_primary = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "HDMI-A-2",
                &[workspace("3", "3", "HDMI-A-2", true, false)],
                Some(window("chat", "Chat")),
                false,
            ),
        ]);
        let restored = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("2", "2", "DP-5", true, false)],
                Some(window("editor", "Editor")),
                false,
            ),
            output(
                "HDMI-A-2",
                &[workspace("3", "3", "HDMI-A-2", true, false)],
                Some(window("chat", "Chat")),
                false,
            ),
        ]);

        assert_eq!(
            surface_specs(&without_primary, &AppConfig::default())[0].output_name,
            "DP-4"
        );
        assert_eq!(
            surface_specs(&restored, &AppConfig::default())[0].output_name,
            "DP-5"
        );
    }

    #[test]
    fn surface_specs_use_output_local_workspaces_and_title() {
        let snapshot = snapshot([
            output(
                "DP-4",
                &[
                    workspace("1", "web", "DP-4", false, false),
                    workspace("2", "term", "DP-4", true, true),
                ],
                Some(window("terminal", "cargo test")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("3", "edit", "DP-5", true, false)],
                Some(window("editor", "nvim")),
                false,
            ),
        ]);

        let specs = surface_specs(&snapshot, &AppConfig::default());
        let reduced = specs
            .iter()
            .find(|spec| spec.output_name == "DP-4")
            .expect("reduced output spec");

        assert_eq!(
            reduced
                .workspaces
                .iter()
                .map(|workspace| workspace.label.as_str())
                .collect::<Vec<_>>(),
            vec!["web", "term"]
        );
        assert_eq!(reduced.title.text, "cargo test");
        assert!(reduced.workspaces[1].active);
        assert!(reduced.workspaces[1].urgent);
    }

    #[test]
    fn surface_specs_hide_reduced_warning_when_context_is_not_critical() {
        let snapshot = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("2", "2", "DP-5", true, false)],
                Some(window("editor", "Editor")),
                false,
            ),
        ]);

        let specs = surface_specs(&snapshot, &AppConfig::default());
        let reduced = specs
            .iter()
            .find(|spec| spec.output_name == "DP-4")
            .expect("reduced output spec");

        assert!(reduced.warning.is_none());
    }

    #[test]
    fn surface_specs_propagate_critical_warning_to_reduced_bars() {
        let mut snapshot = snapshot([
            output(
                "DP-4",
                &[workspace("1", "1", "DP-4", true, false)],
                Some(window("notes", "Notes")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("2", "2", "DP-5", true, false)],
                Some(window("editor", "Editor")),
                false,
            ),
        ]);
        snapshot.system.power = PowerState {
            battery_percent: Some(6),
            charging: false,
            profile: PowerProfile::Balanced,
            changed_at: 0,
        };

        let specs = surface_specs(&snapshot, &AppConfig::default());
        let reduced = specs
            .iter()
            .find(|spec| spec.output_name == "DP-4")
            .expect("reduced output spec");

        assert_eq!(
            reduced.warning.as_ref().map(|warning| warning.tier),
            Some(crate::ContextTier::Critical)
        );
        assert_eq!(
            reduced
                .warning
                .as_ref()
                .map(|warning| warning.text.as_str()),
            Some("Battery 6%")
        );
    }

    #[test]
    fn render_plan_skips_workspace_rebuilds_for_clock_only_changes() {
        let previous = SurfaceSpec {
            output_name: "DP-5".to_string(),
            role: OutputRole::Primary,
            workspaces: vec![super::WorkspaceButtonSpec {
                id: "1".to_string(),
                label: "1".to_string(),
                active: true,
                urgent: false,
            }],
            title: super::TitleSpec {
                text: "Editor".to_string(),
                window_groups: Vec::new(),
            },
            context: None,
            warning: None,
            clock_label: "12:00".to_string(),
        };
        let next = SurfaceSpec {
            clock_label: "12:01".to_string(),
            ..previous.clone()
        };

        let plan = RenderPlan::between(Some(&previous), &next);

        assert!(!plan.workspaces);
        assert!(!plan.title);
        assert!(plan.clock);
    }

    fn snapshot<const N: usize>(outputs: [OutputState; N]) -> BarSnapshot {
        let outputs = outputs
            .into_iter()
            .map(|output| (output.name.clone(), output))
            .collect::<BTreeMap<_, _>>();

        BarSnapshot {
            outputs,
            focused_output: Some("DP-4".to_string()),
            system: crate::SystemState {
                clock: ClockState {
                    epoch_seconds: 1_800_000_000,
                    label: "12:00".to_string(),
                },
                ..crate::SystemState::default()
            },
            ..BarSnapshot::default()
        }
    }

    fn output(
        name: &str,
        workspaces: &[WorkspaceState],
        focused_window: Option<WindowState>,
        urgent: bool,
    ) -> OutputState {
        OutputState {
            name: name.to_string(),
            workspaces: workspaces.to_vec(),
            windows: focused_window.iter().cloned().collect(),
            focused_window,
            urgent,
            changed_at: 0,
        }
    }

    fn workspace(
        id: &str,
        label: &str,
        output: &str,
        active: bool,
        urgent: bool,
    ) -> WorkspaceState {
        WorkspaceState {
            id: id.to_string(),
            label: label.to_string(),
            output: output.to_string(),
            active,
            urgent,
            changed_at: 0,
        }
    }

    fn window(id: &str, title: &str) -> WindowState {
        WindowState {
            id: id.to_string(),
            app_id: None,
            title: title.to_string(),
            urgent: false,
            workspace_id: None,
            changed_at: 0,
        }
    }
}
