use crate::{
    auth::{self, Action, Event},
    config::{Config, State},
    session::{self, Session},
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{
    cell::{Cell, RefCell},
    fs,
    rc::Rc,
    sync::mpsc,
};

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Busy,
    Starting,
    Secret,
    Visible,
    Complete,
}
struct Ui {
    app: gtk::Application,
    config: Config,
    preview: bool,
    sessions: Vec<Session>,
    state: RefCell<State>,
    user_names: Vec<String>,
    account: gtk::DropDown,
    username: gtk::Entry,
    session: gtk::DropDown,
    prompt: gtk::Label,
    secret: gtk::PasswordEntry,
    visible: gtk::Entry,
    status: gtk::Label,
    submit: gtk::Button,
    cancel: gtk::Button,
    spinner: gtk::Spinner,
    power: gtk::Box,
    phase: Cell<Phase>,
    cancelling: Cell<bool>,
    power_busy: Cell<bool>,
    notice: RefCell<String>,
    tx: mpsc::Sender<Action>,
    window: gtk::ApplicationWindow,
    backgrounds: RefCell<Vec<gtk::ApplicationWindow>>,
    monitor_handlers: RefCell<Vec<(gdk::Monitor, glib::SignalHandlerId)>>,
}
fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class(class);
    label
}
fn box_(orientation: gtk::Orientation, spacing: i32) -> gtk::Box {
    gtk::Box::new(orientation, spacing)
}
impl Ui {
    fn user(&self) -> String {
        self.user_names
            .get(self.account.selected() as usize)
            .cloned()
            .unwrap_or_else(|| self.username.text().trim().to_string())
    }
    fn clear(&self) {
        self.secret.set_text("");
        self.visible.set_text("");
    }
    fn message(&self, message: &str, error: bool) {
        self.status.set_label(message);
        if error {
            self.status.add_css_class("error");
        } else {
            self.status.remove_css_class("error");
        }
    }
    fn phase(&self, phase: Phase) {
        self.phase.set(phase);
        let busy = matches!(phase, Phase::Busy | Phase::Starting);
        let complete = phase == Phase::Complete;
        self.account.set_sensitive(!busy && !complete);
        self.session.set_sensitive(!busy && !complete);
        self.username.set_sensitive(!busy && !complete);
        self.secret.set_sensitive(!busy);
        self.visible.set_sensitive(!busy);
        self.submit
            .set_sensitive(!busy && !complete && !self.sessions.is_empty());
        self.cancel.set_sensitive(
            !complete
                && phase != Phase::Starting
                && !self.cancelling.get()
                && !self.power_busy.get(),
        );
        self.power.set_sensitive(!busy && !complete);
        self.spinner.set_spinning(busy);
        self.spinner.set_visible(busy);
        self.submit.set_label(if phase == Phase::Idle {
            "Sign in"
        } else {
            "Continue"
        });
        if !busy {
            self.secret.set_visible(phase == Phase::Secret);
            self.visible.set_visible(phase == Phase::Visible);
            self.prompt
                .set_visible(matches!(phase, Phase::Secret | Phase::Visible));
        }
    }
    fn send(&self, action: Action) {
        self.phase(Phase::Busy);
        if self.tx.send(action).is_err() {
            self.phase(Phase::Idle);
            self.message("Authentication service stopped. Restart the greeter.", true);
        }
    }
    fn begin(&self) {
        if matches!(self.phase.get(), Phase::Busy | Phase::Starting) || self.sessions.is_empty() {
            return;
        }
        let user = self.user();
        if user.is_empty() {
            self.message("Enter your username.", false);
            self.username.grab_focus();
            return;
        }
        self.clear();
        self.notice.borrow_mut().clear();
        self.message("Connecting…", false);
        self.send(Action::Begin(user));
    }
    fn submit(&self) {
        match self.phase.get() {
            Phase::Idle => self.begin(),
            Phase::Secret | Phase::Visible => {
                let answer = if self.phase.get() == Phase::Secret {
                    self.secret.text().to_string()
                } else {
                    self.visible.text().to_string()
                };
                self.clear();
                self.notice.borrow_mut().clear();
                self.message("Authenticating…", false);
                self.send(Action::Answer(answer));
            }
            _ => (),
        }
    }
    fn cancel(&self) {
        if self.power_busy.get()
            || matches!(self.phase.get(), Phase::Starting | Phase::Complete)
            || self.cancelling.replace(true)
        {
            return;
        }
        self.clear();
        self.message("Cancelling…", false);
        self.send(Action::Cancel);
    }
    fn changed_user(&self) {
        self.clear();
        let other = self.account.selected() as usize >= self.user_names.len();
        self.username.set_visible(other);
        let state = self.state.borrow();
        if let Some(index) = session::selected(
            &self.sessions,
            state.sessions.get(&self.user()).map(String::as_str),
            &self.config.initial_session,
        ) {
            self.session.set_selected(index as u32);
        }
        drop(state);
        if other {
            self.cancel();
        } else {
            self.begin();
        }
    }
    fn event(&self, event: Event) {
        if self.cancelling.get() && !matches!(event, Event::Cancelled | Event::Failed(_)) {
            return;
        }
        match event {
            Event::Prompt { secret, message } => {
                self.prompt.set_label(message.trim_end_matches(':'));
                self.clear();
                self.phase(if secret {
                    Phase::Secret
                } else {
                    Phase::Visible
                });
                self.message(&self.notice.borrow(), false);
                if secret {
                    self.secret.grab_focus();
                } else {
                    self.visible.grab_focus();
                }
            }
            Event::Notice(message) => {
                self.notice.replace(message.clone());
                self.message(&message, false);
            }
            Event::Ready => {
                let Some(session) = self.sessions.get(self.session.selected() as usize) else {
                    self.message("No desktop session is available.", true);
                    self.cancel();
                    return;
                };
                self.message("Starting your session…", false);
                self.send(Action::Launch {
                    cmd: session.command(),
                    env: session.environment(),
                });
                self.phase(Phase::Starting);
            }
            Event::Started => {
                self.clear();
                self.phase(Phase::Complete);
                if self.preview {
                    self.message(
                        "Preview complete — no session was started. Close the preview to finish.",
                        false,
                    );
                    return;
                }
                let user = self.user();
                let mut state = self.state.borrow_mut();
                state.user = user.clone();
                if let Some(session) = self.sessions.get(self.session.selected() as usize) {
                    state.sessions.insert(user, session.id.clone());
                }
                if let Err(error) = state.save(&self.config.state) {
                    eprintln!("Could not remember session: {error}");
                }
                self.app.quit();
            }
            Event::Cancelled => {
                self.cancelling.set(false);
                self.clear();
                self.phase(Phase::Idle);
                self.message("", false);
                if WidgetExt::is_visible(&self.username) {
                    self.username.grab_focus();
                } else {
                    self.submit.grab_focus();
                }
            }
            Event::Failed(message) => {
                self.cancelling.set(false);
                self.clear();
                self.phase(Phase::Idle);
                self.message(&message, true);
                self.submit.grab_focus();
            }
        }
    }
    fn resize_panel(&self) {
        if let Some(monitor) = self.window.monitor() {
            self.window
                .set_default_size(monitor.geometry().width().clamp(240, 440), -1);
        }
    }
    fn monitors(self: &Rc<Self>) {
        for (monitor, handler) in self.monitor_handlers.borrow_mut().drain(..) {
            monitor.disconnect(handler);
        }
        let display = gdk::Display::default().unwrap();
        let mut monitors: Vec<gdk::Monitor> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .filter_map(Result::ok)
            .filter(|m| m.is_valid())
            .collect();
        monitors.sort_by_key(|m| m.connector().map(|s| s.to_string()).unwrap_or_default());
        let selected = monitors
            .iter()
            .find(|m| m.connector().as_deref() == Some(&self.config.preferred_output))
            .or(monitors.first());
        // Build replacement surfaces before releasing the old ones.
        let mut backgrounds = Vec::new();
        for monitor in &monitors {
            let weak = Rc::downgrade(self);
            let handler = monitor.connect_geometry_notify(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.resize_panel();
                }
            });
            self.monitor_handlers
                .borrow_mut()
                .push((monitor.clone(), handler));
            let window = gtk::ApplicationWindow::builder()
                .application(&self.app)
                .title("Greeter wallpaper")
                .build();
            window.add_css_class("greeter-background");
            window.init_layer_shell();
            window.set_namespace(Some("shell-greeter-background"));
            window.set_layer(Layer::Background);
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_monitor(Some(monitor));
            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                window.set_anchor(edge, true);
            }
            if let Ok(texture) =
                gdk::Texture::from_file(&gio::File::for_path(&self.config.wallpaper))
            {
                let picture = gtk::Picture::for_paintable(&texture);
                picture.set_content_fit(gtk::ContentFit::Cover);
                picture.set_can_shrink(true);
                window.set_child(Some(&picture));
            }
            window.present();
            backgrounds.push(window);
        }
        if let Some(monitor) = selected {
            self.window.set_monitor(Some(monitor));
            self.window
                .set_default_size(monitor.geometry().width().clamp(240, 440), -1);
            self.window.present();
            match self.phase.get() {
                Phase::Secret => {
                    self.secret.grab_focus();
                }
                Phase::Visible => {
                    self.visible.grab_focus();
                }
                _ => (),
            }
        } else {
            self.window.set_visible(false);
        }
        for old in self.backgrounds.replace(backgrounds) {
            old.close();
        }
    }
}

fn install_css(display: &gdk::Display, config: &Config) {
    let base = gtk::CssProvider::new();
    base.connect_parsing_error(|_, section, error| eprintln!("Greeter CSS: {section:?}: {error}"));
    base.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &base,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    if let Ok(css) = fs::read_to_string(&config.theme) {
        // Generated colors only. A broken generation cannot replace our layout.
        if !css.contains("{{") {
            let theme = gtk::CssProvider::new();
            let valid = Rc::new(Cell::new(true));
            let errors = valid.clone();
            theme.connect_parsing_error(move |_, _, _| errors.set(false));
            theme.load_from_string(&css);
            if valid.get() {
                gtk::style_context_add_provider_for_display(
                    display,
                    &theme,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
                );
            } else {
                eprintln!("Invalid greeter theme; using bundled colors");
            }
        }
    }
    let reduced = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        display,
        &reduced,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
    );
    if let Some(settings) = gtk::Settings::default() {
        let update = move |settings: &gtk::Settings| {
            reduced.load_from_string(if settings.is_gtk_enable_animations() {
                ""
            } else {
                "* { transition: none; animation: none; }"
            })
        };
        update(&settings);
        settings.connect_gtk_enable_animations_notify(update);
    }
}
pub fn start(app: &gtk::Application, config: Config, sessions: Vec<Session>, preview: bool) {
    let display = gdk::Display::default().expect("A Wayland display is required");
    if !gtk4_layer_shell::is_supported() {
        eprintln!("The compositor must support Wayland layer shell");
        app.quit();
        return;
    }
    install_css(&display, &config);
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Sign in")
        .build();
    window.add_css_class("greeter-panel");
    window.init_layer_shell();
    window.set_namespace(Some("shell-greeter"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [
        Edge::Top,
        Edge::Bottom,
        if config.panel_edge == "left" {
            Edge::Left
        } else {
            Edge::Right
        },
    ] {
        window.set_anchor(edge, true);
    }
    let root = box_(gtk::Orientation::Vertical, 24);
    root.add_css_class("panel-content");
    root.set_vexpand(true);
    let header = box_(gtk::Orientation::Vertical, 4);
    let clock = label("", "clock");
    let date = label("", "muted");
    header.append(&clock);
    header.append(&date);
    let tick = move || {
        if let Ok(now) = glib::DateTime::now_local() {
            clock.set_label(
                &now.format("%H:%M")
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            );
            date.set_label(
                &now.format("%A, %e %B")
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            );
        }
        glib::ControlFlow::Continue
    };
    tick();
    glib::timeout_add_seconds_local(1, tick);
    root.append(&header);
    let spacer = box_(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    root.append(&spacer);
    let form = box_(gtk::Orientation::Vertical, 12);
    form.append(&label("Welcome back", "eyebrow"));
    form.append(&label("Sign in", "title"));
    let user_names = session::users();
    let state = if preview {
        State::default()
    } else {
        State::load(&config.state)
    };
    let names: Vec<&str> = user_names
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("Other user…"))
        .collect();
    let account = gtk::DropDown::from_strings(&names);
    account.set_tooltip_text(Some("Account"));
    account.set_selected(
        user_names
            .iter()
            .position(|u| u == &state.user)
            .unwrap_or(0) as u32,
    );
    form.append(&label("ACCOUNT", "caption"));
    form.append(&account);
    let username = gtk::Entry::builder().placeholder_text("Username").build();
    username.set_visible(user_names.is_empty());
    username.update_property(&[gtk::accessible::Property::Label("Username")]);
    form.append(&username);
    let prompt = label("Password", "caption");
    form.append(&prompt);
    let secret = gtk::PasswordEntry::builder().show_peek_icon(true).build();
    secret.update_property(&[gtk::accessible::Property::Label("Authentication response")]);
    form.append(&secret);
    let visible = gtk::Entry::new();
    visible.update_property(&[gtk::accessible::Property::Label("Authentication response")]);
    form.append(&visible);
    let caps = label("Caps Lock is on", "warning");
    caps.set_visible(false);
    if let Some(keyboard) = display.default_seat().and_then(|seat| seat.keyboard()) {
        caps.set_visible(keyboard.is_caps_locked());
        let caps = caps.clone();
        keyboard.connect_caps_lock_state_notify(move |keyboard| {
            caps.set_visible(keyboard.is_caps_locked())
        });
    }
    form.append(&caps);
    let actions = box_(gtk::Orientation::Horizontal, 8);
    let submit = gtk::Button::with_label("Sign in");
    submit.add_css_class("primary");
    submit.set_hexpand(true);
    let cancel = gtk::Button::with_label("Cancel");
    actions.append(&submit);
    actions.append(&cancel);
    form.append(&actions);
    let feedback = box_(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    spinner.set_valign(gtk::Align::Start);
    let status = label("", "status");
    status.set_hexpand(true);
    status.set_lines(4);
    status.set_selectable(false);
    status.set_accessible_role(gtk::AccessibleRole::Status);
    feedback.append(&spinner);
    feedback.append(&status);
    form.append(&feedback);
    root.append(&form);
    let spacer = box_(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    root.append(&spacer);
    let footer = box_(gtk::Orientation::Vertical, 12);
    footer.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    footer.append(&label("DESKTOP SESSION", "caption"));
    let session_names: Vec<String> = sessions
        .iter()
        .map(|s| format!("{} · {}", s.name, if s.x11 { "X11" } else { "Wayland" }))
        .collect();
    let choices: Vec<&str> = session_names.iter().map(String::as_str).collect();
    let session = gtk::DropDown::from_strings(&choices);
    session.set_tooltip_text(Some("Desktop session"));
    session.set_enable_search(true);
    let user = user_names
        .get(account.selected() as usize)
        .map(String::as_str)
        .unwrap_or("");
    if let Some(index) = session::selected(
        &sessions,
        state.sessions.get(user).map(String::as_str),
        &config.initial_session,
    ) {
        session.set_selected(index as u32);
    }
    footer.append(&session);
    let power = box_(gtk::Orientation::Vertical, 8);
    let power_row = box_(gtk::Orientation::Horizontal, 8);
    let confirmation = box_(gtk::Orientation::Vertical, 8);
    confirmation.set_visible(false);
    let confirm_text = label("", "warning");
    let confirm_actions = box_(gtk::Orientation::Horizontal, 8);
    let confirm = gtk::Button::with_label("Confirm");
    confirm.add_css_class("destructive");
    let dismiss = gtk::Button::with_label("Cancel");
    confirm_actions.append(&confirm);
    confirm_actions.append(&dismiss);
    confirmation.append(&confirm_text);
    confirmation.append(&confirm_actions);
    power.append(&power_row);
    power.append(&confirmation);
    footer.append(&power);
    if preview {
        footer.append(&label(
            "PREVIEW · Authentication and power actions are simulated",
            "preview",
        ));
    }
    root.append(&footer);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&root)
        .build();
    window.set_child(Some(&scroll));
    let socket = std::env::var_os("GREETD_SOCK").map(Into::into);
    let (tx, events) = auth::worker(socket, preview);
    let ui = Rc::new(Ui {
        app: app.clone(),
        config,
        preview,
        sessions,
        state: RefCell::new(state),
        user_names,
        account,
        username,
        session,
        prompt,
        secret,
        visible,
        status,
        submit,
        cancel,
        spinner,
        power,
        phase: Cell::new(Phase::Idle),
        cancelling: Cell::new(false),
        power_busy: Cell::new(false),
        notice: RefCell::new(String::new()),
        tx,
        window,
        backgrounds: RefCell::new(Vec::new()),
        monitor_handlers: RefCell::new(Vec::new()),
    });
    for (title, action) in [
        ("Suspend", "suspend"),
        ("Restart", "reboot"),
        ("Shut down", "poweroff"),
    ] {
        let button = gtk::Button::with_label(title);
        button.set_hexpand(true);
        power_row.append(&button);
        let weak = Rc::downgrade(&ui);
        let confirmation = confirmation.clone();
        let confirm_text = confirm_text.clone();
        let confirm = confirm.clone();
        button.connect_clicked(move |_| {
            let Some(ui) = weak.upgrade() else { return };
            ui.clear();
            if action == "suspend" {
                run_power(&ui, action);
            } else {
                confirm_text.set_label(if action == "reboot" {
                    "Restart this computer?"
                } else {
                    "Shut down this computer?"
                });
                confirm.set_label(if action == "reboot" {
                    "Restart"
                } else {
                    "Shut down"
                });
                confirmation.set_widget_name(action);
                confirmation.set_visible(true);
                // Focus cancellation by default, so a repeated Enter cannot confirm.
                if let Some(row) = confirmation.last_child()
                    && let Some(cancel) = row.last_child()
                {
                    // Focus after the newly revealed row has been allocated so
                    // GtkViewport can scroll it into view on short screens.
                    glib::timeout_add_local_once(std::time::Duration::from_millis(40), move || {
                        if cancel.is_visible() {
                            cancel.grab_focus();
                        }
                    });
                }
            }
        });
    }
    let weak = Rc::downgrade(&ui);
    let confirmation_copy = confirmation.clone();
    confirm.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            let action = confirmation_copy.widget_name();
            confirmation_copy.set_visible(false);
            run_power(&ui, &action);
        }
    });
    let confirmation_copy = confirmation.clone();
    let power_focus = power_row.clone();
    dismiss.connect_clicked(move |_| {
        confirmation_copy.set_visible(false);
        if let Some(button) = power_focus.first_child() {
            button.grab_focus();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.submit.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.submit();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.secret.connect_activate(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.submit();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.visible.connect_activate(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.submit();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.username.connect_activate(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.begin();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.cancel.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.cancel();
        }
    });
    let weak = Rc::downgrade(&ui);
    ui.account.connect_selected_notify(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.changed_user();
        }
    });
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_modifiers(move |_, modifiers| {
        caps.set_visible(modifiers.contains(gdk::ModifierType::LOCK_MASK));
        glib::Propagation::Proceed
    });
    let weak = Rc::downgrade(&ui);
    let power_focus = power_row.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            if confirmation.is_visible() {
                confirmation.set_visible(false);
                if let Some(button) = power_focus.first_child() {
                    button.grab_focus();
                }
            } else if let Some(ui) = weak.upgrade() {
                ui.cancel();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    ui.window.add_controller(keys);
    let weak = Rc::downgrade(&ui);
    display.monitors().connect_items_changed(move |_, _, _, _| {
        if let Some(ui) = weak.upgrade() {
            ui.monitors();
        }
    });
    ui.phase(Phase::Idle);
    ui.monitors();
    if ui.sessions.is_empty() {
        ui.message("No desktop sessions are installed.", true);
    } else {
        ui.begin();
    }
    glib::MainContext::default().spawn_local(async move {
        while let Ok(event) = events.recv().await {
            ui.event(event);
        }
    });
}
fn run_power(ui: &Rc<Ui>, action: &str) {
    if !["suspend", "reboot", "poweroff"].contains(&action) {
        return;
    }
    if ui.preview {
        ui.message(&format!("Preview: {action} was not executed."), false);
        return;
    }
    let previous_phase = ui.phase.get();
    ui.power_busy.set(true);
    ui.phase(Phase::Busy);
    ui.message("Requesting power action…", false);
    let action = action.to_string();
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = std::process::Command::new("systemctl")
            .args(["--no-ask-password", &action])
            .output();
        let message = match result {
            Ok(output) if output.status.success() => String::new(),
            Ok(output) => format!(
                "Power action failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(_) => "Could not request the power action.".into(),
        };
        let _ = tx.send_blocking(message);
    });
    let weak = Rc::downgrade(ui);
    glib::MainContext::default().spawn_local(async move {
        if let Ok(message) = rx.recv().await
            && let Some(ui) = weak.upgrade()
        {
            ui.power_busy.set(false);
            ui.phase(previous_phase);
            ui.message(&message, !message.is_empty());
        }
    });
}
