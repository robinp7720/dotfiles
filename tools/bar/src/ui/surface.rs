use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gtk::gdk;
use gtk::gio::prelude::ListModelExtManual;
use gtk::glib;
use gtk::pango::EllipsizeMode;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::{
    ActionCompletion, ActionIntent, ActionRequest, AppConfig, BarSnapshot, CalendarMonthRequest,
    ContextCard, ContextTier, DesktopContext, Direction, Dismissals, MediaControlAction,
    OutputRole, WorkspaceState, select_context,
};

use super::context_card::{context_presentation, context_tier, warning_text};
use super::control_center::{ControlCenterFocus, ControlCenterView};
use super::popovers::PopoverCoordinator;
use super::system::{SystemButtonSpec, SystemCluster, SystemModuleId, build_system_cluster};
use super::wm::{WindowGroupSpec, select_primary_output, title_for_output, window_groups};

const BAR_HEIGHT: i32 = 44;
const SURFACE_MARGIN: i32 = 5;
const WORKSPACE_BUTTON_MIN_WIDTH: i32 = 32;
const WORKSPACE_BUTTON_MIN_HEIGHT: i32 = 28;
const CENTER_SLOT_MIN_WIDTH: i32 = 360;
const CENTER_TEXT_MAX_CHARS: i32 = 56;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceSpec {
    pub output_name: String,
    pub role: OutputRole,
    pub workspaces: Vec<WorkspaceButtonSpec>,
    pub title: TitleSpec,
    pub context: Option<ContextSpec>,
    pub warning: Option<WarningSpec>,
    pub system: Option<SystemCluster>,
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
    pub app: String,
    pub text: String,
    pub window_groups: Vec<WindowGroupSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextSpec {
    pub card: ContextCard,
    pub icon_name: String,
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
    pub system: bool,
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
                system: true,
                clock: true,
            },
            Some(previous) => Self {
                workspaces: previous.workspaces != next.workspaces,
                title: previous.title != next.title,
                context: previous.context != next.context,
                warning: previous.warning != next.warning,
                system: previous.system != next.system,
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
    let primary_context = selected_context.as_ref().map(|card| {
        let presentation = context_presentation(card, now_epoch);
        ContextSpec {
            card: card.clone(),
            icon_name: presentation.icon_name,
            text: presentation.text,
            tier: presentation.tier,
        }
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
            Some(build_system_cluster(snapshot, config)),
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
            None,
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
    system: Option<SystemCluster>,
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
            app: output
                .focused_window
                .as_ref()
                .and_then(|window| window.app_id.clone())
                .filter(|app| !app.trim().is_empty())
                .unwrap_or_else(|| "Desktop".to_string()),
            text: title_for_output(output),
            window_groups,
        },
        context,
        warning,
        system,
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

#[derive(Clone)]
enum ManagedOverlay {
    Popover(gtk::Popover),
    ControlCenter(Rc<ControlCenterView>),
}

impl ManagedOverlay {
    fn hide(&self) {
        match self {
            Self::Popover(popover) => popover.popdown(),
            Self::ControlCenter(control_center) => control_center.dismiss(),
        }
    }

    fn destroy(self) {
        match self {
            Self::Popover(popover) => {
                popover.popdown();
                if popover.parent().is_some() {
                    popover.unparent();
                }
            }
            Self::ControlCenter(control_center) => control_center.destroy(),
        }
    }
}

type PopoverRegistry = Rc<RefCell<BTreeMap<String, ManagedOverlay>>>;

pub struct SurfaceRegistry {
    surfaces: BTreeMap<String, SurfaceHandle>,
    calendar_sender: Option<Sender<CalendarMonthRequest>>,
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
            calendar_sender: None,
        }
    }

    pub fn with_calendar_sender(sender: Sender<CalendarMonthRequest>) -> Self {
        Self {
            surfaces: BTreeMap::new(),
            calendar_sender: Some(sender),
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

            if recreate && let Some(surface) = self.surfaces.remove(&spec.output_name) {
                surface.close();
            }

            match self.surfaces.get_mut(&spec.output_name) {
                Some(surface) => surface.render(&spec),
                None => {
                    let surface = SurfaceHandle::new(
                        application,
                        monitor,
                        &spec,
                        action_sender.clone(),
                        self.calendar_sender.clone(),
                    );
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

    pub fn handle_completion(&mut self, completion: &ActionCompletion) -> bool {
        self.surfaces
            .values_mut()
            .any(|surface| surface.handle_completion(completion))
    }

    pub fn open_control_center(
        &self,
        context: DesktopContext,
        requested_output: Option<&str>,
    ) -> bool {
        let requested = requested_output.and_then(|output| self.surfaces.get(output));
        let primary = requested
            .filter(|surface| matches!(surface, SurfaceHandle::Primary(_)))
            .or_else(|| {
                self.surfaces
                    .values()
                    .find(|surface| matches!(surface, SurfaceHandle::Primary(_)))
            });
        let Some(SurfaceHandle::Primary(surface)) = primary else {
            return false;
        };
        surface.control_center.show_page(context.into());
        surface.control_center.present();
        true
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
        calendar_sender: Option<Sender<CalendarMonthRequest>>,
    ) -> Self {
        match spec.role {
            OutputRole::Primary => Self::Primary(PrimarySurface::new(
                application,
                monitor,
                spec,
                action_sender,
                calendar_sender,
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

    fn handle_completion(&mut self, completion: &ActionCompletion) -> bool {
        match self {
            Self::Primary(surface) => surface.handle_completion(completion),
            Self::Reduced(_) => false,
        }
    }

    fn close(self) {
        match self {
            Self::Primary(surface) => surface.close(),
            Self::Reduced(surface) => surface.close(),
        }
    }
}

pub struct PrimarySurface {
    window: gtk::ApplicationWindow,
    workspaces: gtk::Box,
    app_label: gtk::Label,
    title_label: gtk::Label,
    title_groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    context_stack: gtk::Stack,
    context_row: gtk::Box,
    context_icon: gtk::Image,
    context_label: gtk::Label,
    system_items: gtk::Box,
    status_buttons: BTreeMap<SystemModuleId, StatusButtonView>,
    status_button_order: Vec<SystemModuleId>,
    control_center: Rc<ControlCenterView>,
    popover_coordinator: Rc<RefCell<PopoverCoordinator>>,
    popover_registry: PopoverRegistry,
    action_sender: Sender<ActionRequest>,
    current_spec: Option<SurfaceSpec>,
}

impl PrimarySurface {
    pub fn new(
        application: &gtk::Application,
        monitor: &gdk::Monitor,
        spec: &SurfaceSpec,
        action_sender: Sender<ActionRequest>,
        calendar_sender: Option<Sender<CalendarMonthRequest>>,
    ) -> Self {
        let window = base_window(application, monitor);
        window.add_css_class("bar-window");
        window.add_css_class("primary-bar");
        let popover_coordinator = Rc::new(RefCell::new(PopoverCoordinator::default()));
        let popover_registry = Rc::new(RefCell::new(BTreeMap::new()));

        let root = gtk::CenterBox::new();
        root.add_css_class("bar-root");
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, BAR_HEIGHT);

        let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        left.add_css_class("bar-island");
        left.add_css_class("workspace-island");
        let center_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        center_slot.add_css_class("bar-island");
        center_slot.add_css_class("context-island");
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        left.set_halign(gtk::Align::Start);
        center_slot.set_halign(gtk::Align::Center);
        center_slot.set_hexpand(false);
        center_slot.set_size_request(CENTER_SLOT_MIN_WIDTH, -1);
        right.set_halign(gtk::Align::End);

        let workspaces = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        workspaces.add_css_class("workspace-strip");
        install_workspace_scroll(&workspaces, action_sender.clone(), spec.output_name.clone());
        left.append(&workspaces);
        let app_label = gtk::Label::new(None);
        app_label.add_css_class("app-label");
        app_label.set_max_width_chars(22);
        app_label.set_ellipsize(EllipsizeMode::End);
        left.append(&app_label);

        let title_button = gtk::Button::new();
        title_button.add_css_class("title-button");
        title_button.set_has_frame(false);
        title_button.set_hexpand(true);
        title_button.set_halign(gtk::Align::Fill);
        let title_label = gtk::Label::new(None);
        title_label.add_css_class("title-label");
        title_label.set_hexpand(false);
        title_label.set_max_width_chars(48);
        title_label.set_ellipsize(EllipsizeMode::End);
        title_label.set_xalign(0.5);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        title_row.set_hexpand(false);
        title_row.set_halign(gtk::Align::Center);
        let title_icon = gtk::Image::from_icon_name("focus-windows-symbolic");
        title_icon.add_css_class("context-icon");
        title_icon.set_pixel_size(16);
        title_icon.set_size_request(16, 16);
        title_row.append(&title_icon);
        title_row.append(&title_label);
        title_button.set_child(Some(&title_row));

        let title_groups = Rc::new(RefCell::new(Vec::new()));
        install_title_interactions(
            &title_button,
            title_groups.clone(),
            action_sender.clone(),
            spec.output_name.clone(),
            popover_coordinator.clone(),
            popover_registry.clone(),
        );

        let context_stack = gtk::Stack::new();
        context_stack.set_halign(gtk::Align::Fill);
        context_stack.set_hexpand(true);
        context_stack.set_hhomogeneous(false);
        context_stack.set_vhomogeneous(true);
        context_stack.set_transition_duration(180);
        context_stack.set_transition_type(context_transition_type());
        let context_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        context_row.add_css_class("context-chip");
        context_row.set_hexpand(false);
        context_row.set_halign(gtk::Align::Center);
        let context_icon = gtk::Image::new();
        context_icon.add_css_class("context-icon");
        context_icon.set_pixel_size(16);
        context_icon.set_size_request(16, 16);
        let context_label = gtk::Label::new(None);
        context_label.set_hexpand(false);
        context_label.set_xalign(0.5);
        context_label.set_max_width_chars(CENTER_TEXT_MAX_CHARS);
        context_label.set_ellipsize(EllipsizeMode::End);
        context_row.append(&context_icon);
        context_row.append(&context_label);
        context_stack.add_named(&title_button, Some("title"));
        context_stack.add_named(&context_row, Some("context"));
        context_stack.set_visible_child_name("title");

        center_slot.append(&context_stack);

        let system_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        system_box.add_css_class("bar-island");
        system_box.add_css_class("system-cluster");
        right.append(&system_box);
        let system_items = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        system_items.add_css_class("system-items");
        system_box.append(&system_items);

        let initial_system = spec.system.as_ref().expect("primary system spec");
        let control_center = Rc::new(ControlCenterView::new(
            application,
            monitor,
            &spec.output_name,
            SURFACE_MARGIN,
            SURFACE_MARGIN,
            initial_system.control_center(),
            action_sender.clone(),
            calendar_sender,
        ));
        register_control_window(
            "control-center",
            control_center.clone(),
            popover_coordinator.clone(),
            popover_registry.clone(),
        );
        let mut status_buttons = BTreeMap::new();
        for module in initial_system.modules() {
            let status_button = build_status_button(
                &module.button,
                &action_sender,
                control_center.clone(),
                popover_coordinator.clone(),
                popover_registry.clone(),
            );
            system_items.append(&status_button.button);
            status_buttons.insert(module.button.id, status_button);
        }
        let status_button_order = initial_system
            .modules()
            .iter()
            .map(|module| module.button.id)
            .collect();

        root.set_start_widget(Some(&left));
        root.set_center_widget(Some(&center_slot));
        root.set_end_widget(Some(&right));

        window.set_child(Some(&root));
        install_escape_dismiss(
            &window,
            popover_coordinator.clone(),
            popover_registry.clone(),
        );
        window.present();

        let mut surface = Self {
            window,
            workspaces,
            app_label,
            title_label,
            title_groups,
            context_stack,
            context_row,
            context_icon,
            context_label,
            system_items,
            status_buttons,
            status_button_order,
            control_center,
            popover_coordinator,
            popover_registry,
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
            self.app_label.set_label(&spec.title.app);
            self.title_label.set_label(&spec.title.text);
            *self.title_groups.borrow_mut() = spec.title.window_groups.clone();
        }
        if plan.system
            && let Some(system) = spec.system.as_ref()
        {
            self.render_system_modules(system);
        }

        if plan.context {
            if let Some(context) = spec.context.as_ref() {
                self.context_icon.set_icon_name(Some(&context.icon_name));
                self.context_label.set_label(&context.text);
                apply_tier_classes(&self.context_row, context.tier);
                self.context_stack.set_visible_child_name("context");
            } else {
                self.context_icon.set_icon_name(None);
                self.context_label.set_label("");
                clear_tier_classes(&self.context_row);
                self.context_stack.set_visible_child_name("title");
            }
        }

        self.current_spec = Some(spec.clone());
    }

    pub fn handle_completion(&mut self, completion: &ActionCompletion) -> bool {
        self.control_center.handle_completion(completion)
    }

    fn close(self) {
        destroy_popovers(&self.popover_registry);
        self.window.close();
    }

    fn render_system_modules(&mut self, system: &SystemCluster) {
        self.control_center.update(system.control_center());

        let new_order: Vec<SystemModuleId> = system
            .modules()
            .iter()
            .map(|module| module.button.id)
            .collect();
        if new_order != self.status_button_order {
            self.rebuild_system_items(system);
            return;
        }

        for module in system.modules() {
            if let Some(status_button) = self.status_buttons.get_mut(&module.button.id) {
                status_button.update(&module.button);
            }
        }
    }

    fn rebuild_system_items(&mut self, system: &SystemCluster) {
        while let Some(child) = self.system_items.first_child() {
            self.system_items.remove(&child);
        }
        self.status_buttons.clear();

        for module in system.modules() {
            let status_button = build_status_button(
                &module.button,
                &self.action_sender,
                self.control_center.clone(),
                self.popover_coordinator.clone(),
                self.popover_registry.clone(),
            );
            self.system_items.append(&status_button.button);
            self.status_buttons.insert(module.button.id, status_button);
        }
        self.status_button_order = system
            .modules()
            .iter()
            .map(|module| module.button.id)
            .collect();
    }
}

pub struct ReducedSurface {
    window: gtk::ApplicationWindow,
    workspaces: gtk::Box,
    app_label: gtk::Label,
    title_label: gtk::Label,
    title_groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    warning_label: gtk::Label,
    clock_label: gtk::Label,
    popover_registry: PopoverRegistry,
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
        window.add_css_class("bar-window");
        window.add_css_class("reduced-bar");
        let popover_coordinator = Rc::new(RefCell::new(PopoverCoordinator::default()));
        let popover_registry = Rc::new(RefCell::new(BTreeMap::new()));

        let root = gtk::Grid::new();
        root.add_css_class("bar-root");
        root.set_column_spacing(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, BAR_HEIGHT);

        let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        left.add_css_class("bar-island");
        left.add_css_class("workspace-island");
        left.set_hexpand(true);
        left.set_halign(gtk::Align::Start);
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        right.add_css_class("bar-island");
        right.add_css_class("reduced-status-island");
        right.set_halign(gtk::Align::End);

        let workspaces = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        workspaces.add_css_class("workspace-strip");
        install_workspace_scroll(&workspaces, action_sender.clone(), spec.output_name.clone());
        left.append(&workspaces);
        let app_label = gtk::Label::new(None);
        app_label.add_css_class("app-label");
        app_label.set_max_width_chars(18);
        app_label.set_ellipsize(EllipsizeMode::End);
        left.append(&app_label);
        let title_button = gtk::Button::new();
        title_button.add_css_class("title-button");
        title_button.set_has_frame(false);
        title_button.set_hexpand(true);
        title_button.set_halign(gtk::Align::Fill);
        let title_label = gtk::Label::new(None);
        title_label.add_css_class("title-label");
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
            popover_coordinator.clone(),
            popover_registry.clone(),
        );

        let warning_label = gtk::Label::new(None);
        warning_label.add_css_class("warning-chip");
        warning_label.set_visible(false);
        warning_label.set_ellipsize(EllipsizeMode::End);

        let clock_label = gtk::Label::new(None);
        clock_label.add_css_class("clock-chip");

        left.append(&title_button);
        right.append(&warning_label);
        right.append(&clock_label);
        root.attach(&left, 0, 0, 1, 1);
        root.attach(&right, 1, 0, 1, 1);

        window.set_child(Some(&root));
        install_escape_dismiss(&window, popover_coordinator, popover_registry.clone());
        window.present();

        let mut surface = Self {
            window,
            workspaces,
            app_label,
            title_label,
            title_groups,
            warning_label,
            clock_label,
            popover_registry,
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
            self.app_label.set_label(&spec.title.app);
            self.title_label.set_label(&spec.title.text);
            *self.title_groups.borrow_mut() = spec.title.window_groups.clone();
        }
        if plan.clock {
            self.clock_label.set_label(&spec.clock_label);
        }

        if plan.warning {
            if let Some(warning) = spec.warning.as_ref() {
                self.warning_label.set_label(&warning.text);
                apply_tier_classes(&self.warning_label, warning.tier);
                self.warning_label.set_visible(true);
            } else {
                self.warning_label.set_label("");
                clear_tier_classes(&self.warning_label);
                self.warning_label.set_visible(false);
            }
        }

        self.current_spec = Some(spec.clone());
    }

    fn close(self) {
        destroy_popovers(&self.popover_registry);
        self.window.close();
    }
}

fn base_window(application: &gtk::Application, monitor: &gdk::Monitor) -> gtk::ApplicationWindow {
    let width = bar_window_width_for_monitor_width(monitor.geometry().width());
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("cockpit-bar")
        .build();
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(width, BAR_HEIGHT);
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

fn bar_window_width_for_monitor_width(monitor_width: i32) -> i32 {
    monitor_width.saturating_sub(SURFACE_MARGIN * 2).max(1)
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
        button.add_css_class("workspace-button");
        update_state_class(&button, "active", workspace.active);
        update_state_class(&button, "urgent", workspace.urgent);
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

fn install_workspace_scroll(
    container: &gtk::Box,
    action_sender: Sender<ActionRequest>,
    output_name: String,
) {
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL
            | gtk::EventControllerScrollFlags::HORIZONTAL
            | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(move |_, dx, dy| {
        let Some(direction) = scroll_direction(dx, dy) else {
            return glib::Propagation::Proceed;
        };

        let _ = action_sender.send(ActionRequest {
            origin: format!("workspace-scroll:{output_name}"),
            intent: ActionIntent::CycleWorkspace {
                output: output_name.clone(),
                direction,
            },
        });
        glib::Propagation::Stop
    });
    container.add_controller(scroll);
}

fn install_title_interactions(
    button: &gtk::Button,
    groups: Rc<RefCell<Vec<WindowGroupSpec>>>,
    action_sender: Sender<ActionRequest>,
    output_name: String,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_parent(button);
    let popover_id = format!("title:{output_name}");
    register_popover(&popover_id, &popover, coordinator.clone(), registry.clone());

    let groups_for_click = groups.clone();
    let sender_for_click = action_sender.clone();
    let popover_output = output_name.clone();
    let popover_id_for_click = popover_id.clone();
    button.connect_clicked(move |_| {
        rebuild_window_popover(
            &popover,
            &groups_for_click.borrow(),
            &sender_for_click,
            &popover_output,
        );
        show_managed_popover(
            &popover_id_for_click,
            &popover,
            coordinator.clone(),
            registry.clone(),
        );
    });

    let secondary = gtk::GestureClick::new();
    secondary.set_button(3);
    let secondary_output = output_name.clone();
    secondary.connect_pressed(move |_, _, _, _| {
        let _ = action_sender.send(ActionRequest {
            origin: format!("title-secondary:{secondary_output}"),
            intent: ActionIntent::OpenWindowSearch {
                output: secondary_output.clone(),
            },
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

fn apply_tier_classes(widget: &impl IsA<gtk::Widget>, tier: crate::ContextTier) {
    clear_tier_classes(widget);
    match tier {
        crate::ContextTier::Critical => widget.add_css_class("critical"),
        crate::ContextTier::Imminent => widget.add_css_class("warning"),
        crate::ContextTier::Work => widget.add_css_class("active"),
        crate::ContextTier::Ambient => {}
    }
}

fn clear_tier_classes(widget: &impl IsA<gtk::Widget>) {
    widget.remove_css_class("active");
    widget.remove_css_class("warning");
    widget.remove_css_class("critical");
}

fn update_state_class(widget: &impl IsA<gtk::Widget>, class_name: &str, present: bool) {
    if present {
        widget.add_css_class(class_name);
    } else {
        widget.remove_css_class(class_name);
    }
}

struct StatusButtonView {
    button: gtk::Button,
    icon: gtk::Image,
    label: gtk::Label,
    classes: Vec<String>,
}

impl StatusButtonView {
    fn update(&mut self, spec: &SystemButtonSpec) {
        self.button.set_tooltip_text(Some(&spec.tooltip));
        for class_name in std::mem::take(&mut self.classes) {
            self.button.remove_css_class(&class_name);
        }
        for class_name in &spec.classes {
            self.button.add_css_class(class_name);
        }
        self.classes = spec.classes.clone();
        self.icon.set_icon_name(Some(&spec.icon_name));
        if let Some(text) = spec.label.as_deref() {
            self.label.set_label(text);
            self.label.set_visible(true);
        } else {
            self.label.set_label("");
            self.label.set_visible(false);
        }
    }
}

fn build_status_button(
    button_spec: &SystemButtonSpec,
    action_sender: &Sender<ActionRequest>,
    control_center: Rc<ControlCenterView>,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) -> StatusButtonView {
    let button = gtk::Button::new();
    button.set_has_frame(false);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let icon = gtk::Image::new();
    let label = gtk::Label::new(None);
    row.append(&icon);
    row.append(&label);
    button.set_child(Some(&row));

    let focus = control_focus(button_spec.id);
    let center_for_click = control_center.clone();
    button.connect_clicked(move |_| {
        if center_for_click.is_visible() && center_for_click.current_page() == focus {
            center_for_click.dismiss();
            return;
        }
        center_for_click.show_page(focus);
        show_managed_window(
            "control-center",
            center_for_click.clone(),
            coordinator.clone(),
            registry.clone(),
        );
    });

    if button_spec.id == SystemModuleId::Audio {
        install_media_scroll(
            &button,
            action_sender.clone(),
            control_center,
            format!("scroll:{}", button_spec.id.as_str()),
        );
    } else if let Some((previous, next)) = scroll_actions(button_spec.id) {
        install_action_scroll(
            &button,
            action_sender.clone(),
            previous,
            next,
            format!("scroll:{}", button_spec.id.as_str()),
        );
    }

    let mut view = StatusButtonView {
        button,
        icon,
        label,
        classes: Vec::new(),
    };
    view.update(button_spec);
    view
}

fn control_focus(module_id: SystemModuleId) -> ControlCenterFocus {
    match module_id {
        SystemModuleId::Keyboard => ControlCenterFocus::Keyboard,
        SystemModuleId::Resources => ControlCenterFocus::Resources,
        SystemModuleId::Network => ControlCenterFocus::Network,
        SystemModuleId::Audio => ControlCenterFocus::Audio,
        SystemModuleId::Power => ControlCenterFocus::Power,
        SystemModuleId::Clock => ControlCenterFocus::Clock,
    }
}

fn install_action_scroll(
    widget: &impl IsA<gtk::Widget>,
    action_sender: Sender<ActionRequest>,
    previous: ActionIntent,
    next: ActionIntent,
    origin: String,
) {
    install_direction_scroll(widget, action_sender, origin, move |direction| {
        Some(match direction {
            Direction::Previous => previous.clone(),
            Direction::Next => next.clone(),
        })
    });
}

fn install_media_scroll(
    widget: &impl IsA<gtk::Widget>,
    action_sender: Sender<ActionRequest>,
    control_center: Rc<ControlCenterView>,
    origin: String,
) {
    install_direction_scroll(widget, action_sender, origin, move |direction| {
        control_center
            .media_player()
            .map(|player| ActionIntent::ControlMedia {
                player,
                action: match direction {
                    Direction::Previous => MediaControlAction::Previous,
                    Direction::Next => MediaControlAction::Next,
                },
            })
    });
}

fn install_direction_scroll<F>(
    widget: &impl IsA<gtk::Widget>,
    action_sender: Sender<ActionRequest>,
    origin: String,
    intent_for_direction: F,
) where
    F: Fn(Direction) -> Option<ActionIntent> + 'static,
{
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL
            | gtk::EventControllerScrollFlags::HORIZONTAL
            | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(move |_, dx, dy| {
        let Some(direction) = scroll_direction(dx, dy) else {
            return glib::Propagation::Proceed;
        };
        let Some(intent) = intent_for_direction(direction) else {
            return glib::Propagation::Proceed;
        };
        let _ = action_sender.send(ActionRequest {
            origin: origin.clone(),
            intent,
        });
        glib::Propagation::Stop
    });
    widget.add_controller(scroll);
}

fn register_popover(
    popover_id: &str,
    popover: &gtk::Popover,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    registry.borrow_mut().insert(
        popover_id.to_string(),
        ManagedOverlay::Popover(popover.clone()),
    );
    let popover_id = popover_id.to_string();
    popover.connect_closed(move |_| {
        coordinator.borrow_mut().close(&popover_id);
    });
}

fn register_control_window(
    overlay_id: &str,
    control_center: Rc<ControlCenterView>,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    registry.borrow_mut().insert(
        overlay_id.to_string(),
        ManagedOverlay::ControlCenter(control_center.clone()),
    );
    let overlay_id = overlay_id.to_string();
    control_center
        .window()
        .connect_visible_notify(move |window| {
            if !window.is_visible() {
                coordinator.borrow_mut().close(&overlay_id);
            }
        });
}

fn show_managed_popover(
    popover_id: &str,
    popover: &gtk::Popover,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    if let Some(previous) = coordinator.borrow_mut().open(popover_id)
        && previous != popover_id
        && let Some(active) = registry.borrow().get(&previous).cloned()
    {
        active.hide();
    }
    if !popover.is_visible() {
        popover.popup();
    }
}

fn show_managed_window(
    overlay_id: &str,
    control_center: Rc<ControlCenterView>,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    if let Some(previous) = coordinator.borrow_mut().open(overlay_id)
        && previous != overlay_id
        && let Some(active) = registry.borrow().get(&previous).cloned()
    {
        active.hide();
    }
    control_center.present();
}

fn popdown_active_popover(
    coordinator: &Rc<RefCell<PopoverCoordinator>>,
    registry: &PopoverRegistry,
) {
    if let Some(active) = coordinator.borrow_mut().clear_active()
        && let Some(overlay) = registry.borrow().get(&active).cloned()
    {
        overlay.hide();
    }
}

fn install_escape_dismiss(
    window: &gtk::ApplicationWindow,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            popdown_active_popover(&coordinator, &registry);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
}

fn destroy_popovers(registry: &PopoverRegistry) {
    let overlays = std::mem::take(&mut *registry.borrow_mut());
    for (_, overlay) in overlays {
        overlay.destroy();
    }
}

fn scroll_direction(dx: f64, dy: f64) -> Option<Direction> {
    let delta = if dy.abs() >= dx.abs() { dy } else { dx };
    if delta < 0.0 {
        Some(Direction::Previous)
    } else if delta > 0.0 {
        Some(Direction::Next)
    } else {
        None
    }
}

fn scroll_actions(module_id: SystemModuleId) -> Option<(ActionIntent, ActionIntent)> {
    match module_id {
        SystemModuleId::Keyboard => Some((
            ActionIntent::ToggleKeyboardLayout,
            ActionIntent::ToggleKeyboardLayout,
        )),
        SystemModuleId::Audio => None,
        SystemModuleId::Power => Some((
            ActionIntent::CyclePowerProfile {
                direction: Direction::Previous,
            },
            ActionIntent::CyclePowerProfile {
                direction: Direction::Next,
            },
        )),
        SystemModuleId::Resources | SystemModuleId::Network | SystemModuleId::Clock => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        AppConfig, BarSnapshot, ClockState, OutputRole, OutputState, PowerProfile, PowerState,
        WindowState, WorkspaceState, reload_runtime_config,
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
    fn surface_specs_recalculate_primary_role_after_runtime_reload() {
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
        let current = AppConfig::default();
        let mut next = current.clone();
        next.primary_output = Some("DP-4".to_string());

        let reloaded = reload_runtime_config(&current, next);
        let specs = surface_specs(&snapshot, &reloaded.config);

        assert_eq!(specs[0].output_name, "DP-4");
        assert_eq!(specs[0].role, OutputRole::Primary);
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
        assert_eq!(reduced.title.app, "terminal");
        assert!(reduced.workspaces[1].active);
        assert!(reduced.workspaces[1].urgent);
    }

    #[test]
    fn surface_specs_keep_reduced_outputs_free_of_system_modules() {
        let snapshot = snapshot([
            output(
                "DP-4",
                &[workspace("1", "web", "DP-4", false, false)],
                Some(window("terminal", "cargo test")),
                false,
            ),
            output(
                "DP-5",
                &[workspace("2", "edit", "DP-5", true, false)],
                Some(window("editor", "nvim")),
                false,
            ),
        ]);

        let specs = surface_specs(&snapshot, &AppConfig::default());
        let primary = specs
            .iter()
            .find(|spec| spec.output_name == "DP-5")
            .expect("primary output spec");
        let reduced = specs
            .iter()
            .find(|spec| spec.output_name == "DP-4")
            .expect("reduced output spec");

        assert!(primary.system.is_some());
        assert_eq!(reduced.system, None);
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
            battery_present: true,
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
                app: "editor".to_string(),
                text: "Editor".to_string(),
                window_groups: Vec::new(),
            },
            context: None,
            warning: None,
            system: None,
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

    #[test]
    fn bar_window_width_matches_monitor_width_minus_layer_margins() {
        assert_eq!(super::bar_window_width_for_monitor_width(3_840), 3_830);
        assert_eq!(super::bar_window_width_for_monitor_width(1), 1);
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
            app_id: Some(id.to_string()),
            title: title.to_string(),
            urgent: false,
            workspace_id: None,
            changed_at: 0,
        }
    }
}
