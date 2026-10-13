use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use gilrs::{Axis, Button, EventType, Gilrs};
use gtk::gdk;
use gtk::gio::prelude::ListModelExtManual;
use gtk::glib::{self, ControlFlow, Propagation};
use gtk::prelude::*;
use gtk4 as gtk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

use crate::config::{CommandSpec, Config};
use crate::control::{self, ControlCommand};
use crate::session::{Profile, Session};

const POWER_HOLD: Duration = Duration::from_secs(2);
const GUIDE_HOLD: Duration = Duration::from_millis(1500);
const APP_START_GRACE: Duration = Duration::from_secs(30);
const APP_EXIT_STABLE: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppKind {
    Steam,
    Kodi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug)]
enum UiEvent {
    Navigate(NavDirection),
    ActivatePressed,
    ActivateReleased,
    Back,
    Show,
    ExitRequested,
    AppExited(AppKind),
    Controller(String),
    Error(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tile {
    Steam,
    Kodi,
    Sleep,
    Shutdown,
    Exit,
}

impl Tile {
    const ALL: [Self; 5] = [
        Self::Steam,
        Self::Kodi,
        Self::Sleep,
        Self::Shutdown,
        Self::Exit,
    ];

    fn is_power(self) -> bool {
        matches!(self, Self::Sleep | Self::Shutdown)
    }
}

struct UiState {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    buttons: Vec<gtk::Button>,
    status: gtk::Label,
    controller: gtk::Label,
    selected: Cell<usize>,
    active_app: Cell<Option<AppKind>>,
    hold: RefCell<Option<(Tile, Instant)>>,
    hold_completed: Cell<bool>,
    session: Rc<RefCell<Option<Session>>>,
    events: Sender<UiEvent>,
}

pub fn run(session: Session) -> Result<()> {
    let config = session.config().clone();
    let session = Rc::new(RefCell::new(Some(session)));
    let (event_tx, event_rx) = mpsc::channel::<UiEvent>();
    let (control_tx, control_rx) = mpsc::channel::<ControlCommand>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let control_join = control::spawn_server(control_tx, Arc::clone(&cancelled))?;
    let gamepad_join = spawn_gamepad_worker(event_tx.clone(), Arc::clone(&cancelled));

    let application = gtk::Application::builder()
        .application_id("me.robindecker.TvMode")
        .build();
    let state_for_activate = Rc::clone(&session);
    let tx_for_activate = event_tx.clone();
    let config_for_activate = config.clone();
    let receiver = Rc::new(RefCell::new(Some((event_rx, control_rx))));
    let receiver_for_activate = Rc::clone(&receiver);

    application.connect_activate(move |app| {
        let Some((event_rx, control_rx)) = receiver_for_activate.borrow_mut().take() else {
            return;
        };
        match build_ui(
            app,
            &config_for_activate,
            Rc::clone(&state_for_activate),
            tx_for_activate.clone(),
            event_rx,
            control_rx,
        ) {
            Ok(state) => state.window.present(),
            Err(error) => {
                eprintln!("failed to build TV mode UI: {error:#}");
                app.quit();
            }
        }
    });

    // Clap already consumed the binary's `run` subcommand. Do not let
    // GApplication reinterpret it as a file to open.
    application.run_with_args(&["tv-mode"]);
    cancelled.store(true, Ordering::Relaxed);
    let _ = control_join.join();
    let _ = gamepad_join.join();

    if let Some(mut session) = session.borrow_mut().take() {
        session.restore()?;
    }
    Ok(())
}

fn build_ui(
    app: &gtk::Application,
    config: &Config,
    session: Rc<RefCell<Option<Session>>>,
    event_tx: Sender<UiEvent>,
    event_rx: Receiver<UiEvent>,
    control_rx: Receiver<ControlCommand>,
) -> Result<Rc<UiState>> {
    load_css()?;
    let monitor = monitor_for_connector(&config.output)
        .ok_or_else(|| anyhow!("GDK could not find output {}", config.output))?;
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("TV Home")
        .build();
    window.set_decorated(false);
    window.init_layer_shell();
    window.set_namespace(Some("tv-mode"));
    window.set_layer(Layer::Overlay);
    window.set_monitor(Some(&monitor));
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.add_css_class("tv-shell");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 28);
    root.add_css_class("tv-root");
    if !gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()) {
        root.add_css_class("reduced-motion");
    }
    root.set_margin_top(72);
    root.set_margin_bottom(72);
    root.set_margin_start(96);
    root.set_margin_end(96);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let eyebrow = gtk::Label::new(Some("LIVING ROOM"));
    eyebrow.add_css_class("tv-eyebrow");
    eyebrow.set_xalign(0.0);
    let title = gtk::Label::new(Some("What are we playing?"));
    title.add_css_class("tv-title");
    title.set_xalign(0.0);
    title_box.append(&eyebrow);
    title_box.append(&title);
    title_box.set_hexpand(true);
    let clock = gtk::Label::new(None);
    clock.add_css_class("tv-clock");
    update_clock(&clock);
    header.append(&title_box);
    header.append(&clock);
    root.append(&header);

    let hero = gtk::Grid::new();
    hero.set_column_spacing(28);
    hero.set_row_spacing(28);
    hero.set_vexpand(true);
    hero.set_valign(gtk::Align::Center);

    let steam = tile_button("", "Steam", "Big Picture • 1080p 120 Hz", "hero steam");
    let kodi = tile_button("󰟴", "Kodi", "Media center • 4K 60 Hz", "hero kodi");
    let sleep = tile_button("󰒲", "Sleep", "Hold to lock and suspend", "utility");
    let shutdown = tile_button("󰐥", "Shut down", "Hold to power off", "utility danger");
    let exit = tile_button(
        "󰍃",
        "Exit TV mode",
        "Restore desk displays and audio",
        "utility exit",
    );
    hero.attach(&steam, 0, 0, 1, 1);
    hero.attach(&kodi, 1, 0, 1, 1);
    hero.attach(&sleep, 0, 1, 1, 1);
    hero.attach(&shutdown, 1, 1, 1, 1);
    hero.attach(&exit, 2, 1, 1, 1);
    root.append(&hero);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    let status = gtk::Label::new(Some("TV audio • 4K 60 Hz"));
    status.add_css_class("tv-status");
    status.set_xalign(0.0);
    status.set_hexpand(true);
    let controller = gtk::Label::new(Some("Connect a controller • Arrow keys also work"));
    controller.add_css_class("tv-controller");
    controller.set_xalign(1.0);
    footer.append(&status);
    footer.append(&controller);
    root.append(&footer);
    window.set_child(Some(&root));

    let buttons = vec![steam, kodi, sleep, shutdown, exit];
    let state = Rc::new(UiState {
        app: app.clone(),
        window,
        buttons,
        status,
        controller,
        selected: Cell::new(0),
        active_app: Cell::new(None),
        hold: RefCell::new(None),
        hold_completed: Cell::new(false),
        session,
        events: event_tx,
    });

    wire_buttons(&state);
    wire_keyboard(&state);
    state.buttons[0].grab_focus();

    let state_for_events = Rc::clone(&state);
    glib::timeout_add_local(Duration::from_millis(40), move || {
        loop {
            match event_rx.try_recv() {
                Ok(event) => handle_event(&state_for_events, event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        loop {
            match control_rx.try_recv() {
                Ok(ControlCommand::Show) => handle_event(&state_for_events, UiEvent::Show),
                Ok(ControlCommand::Exit) => handle_event(&state_for_events, UiEvent::ExitRequested),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        update_hold(&state_for_events);
        ControlFlow::Continue
    });

    let clock_for_tick = clock.clone();
    glib::timeout_add_local(Duration::from_secs(1), move || {
        update_clock(&clock_for_tick);
        ControlFlow::Continue
    });

    Ok(state)
}

fn wire_buttons(state: &Rc<UiState>) {
    for (index, button) in state.buttons.iter().enumerate() {
        let state_for_focus = Rc::clone(state);
        button.connect_has_focus_notify(move |button| {
            if button.has_focus() {
                state_for_focus.selected.set(index);
            }
        });

        let tile = Tile::ALL[index];
        let state_for_click = Rc::clone(state);
        button.connect_clicked(move |_| {
            if tile.is_power() {
                state_for_click
                    .status
                    .set_text("Hold A or Enter for two seconds to confirm");
            } else {
                activate_tile(&state_for_click, tile);
            }
        });
    }
}

fn wire_keyboard(state: &Rc<UiState>) {
    let keys = gtk::EventControllerKey::new();
    let state_for_press = Rc::clone(state);
    keys.connect_key_pressed(move |_, key, _, _| {
        let event = match key {
            gdk::Key::Left => Some(UiEvent::Navigate(NavDirection::Left)),
            gdk::Key::Right => Some(UiEvent::Navigate(NavDirection::Right)),
            gdk::Key::Up => Some(UiEvent::Navigate(NavDirection::Up)),
            gdk::Key::Down => Some(UiEvent::Navigate(NavDirection::Down)),
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                Some(UiEvent::ActivatePressed)
            }
            gdk::Key::Escape => Some(UiEvent::Back),
            _ => None,
        };
        if let Some(event) = event {
            handle_event(&state_for_press, event);
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    let state_for_release = Rc::clone(state);
    keys.connect_key_released(move |_, key, _, _| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space) {
            handle_event(&state_for_release, UiEvent::ActivateReleased);
        }
    });
    state.window.add_controller(keys);
}

fn handle_event(state: &Rc<UiState>, event: UiEvent) {
    match event {
        UiEvent::Navigate(direction) if state.window.is_visible() => navigate(state, direction),
        UiEvent::ActivatePressed if state.window.is_visible() => {
            let tile = Tile::ALL[state.selected.get()];
            if tile.is_power() {
                state.hold_completed.set(false);
                state.hold.replace(Some((tile, Instant::now())));
                state.status.set_text("Keep holding to confirm…");
            } else {
                activate_tile(state, tile);
            }
        }
        UiEvent::ActivateReleased => {
            if !state.hold_completed.replace(false) && state.hold.borrow().is_some() {
                state.status.set_text("Power action cancelled");
            }
            state.hold.replace(None);
        }
        UiEvent::Back => {
            state.hold.replace(None);
            if state.active_app.get().is_some() {
                state.window.set_visible(false);
            }
        }
        UiEvent::Show => {
            state.window.present();
            state.buttons[state.selected.get()].grab_focus();
            if state.active_app.get().is_some() {
                state
                    .status
                    .set_text("Entertainment is still active • B returns to it");
            }
        }
        UiEvent::ExitRequested => request_exit(state),
        UiEvent::AppExited(kind) if state.active_app.get() == Some(kind) => {
            state.active_app.set(None);
            if let Some(session) = state.session.borrow().as_ref()
                && let Err(error) = session.apply_profile(Profile::Home)
            {
                state
                    .status
                    .set_text(&format!("Failed to restore home profile: {error}"));
            }
            state.status.set_text("TV audio • 4K 60 Hz");
            state.window.present();
            state.buttons[0].grab_focus();
        }
        UiEvent::Controller(name) => state.controller.set_text(&format!(
            "{name} • D-pad navigate • A select • hold Guide for home"
        )),
        UiEvent::Error(message) => state.status.set_text(&message),
        _ => {}
    }
}

fn activate_tile(state: &Rc<UiState>, tile: Tile) {
    match tile {
        Tile::Steam => launch_app(state, AppKind::Steam),
        Tile::Kodi => launch_app(state, AppKind::Kodi),
        Tile::Exit => request_exit(state),
        Tile::Sleep | Tile::Shutdown => state
            .status
            .set_text("Hold A or Enter for two seconds to confirm"),
    }
}

fn launch_app(state: &Rc<UiState>, kind: AppKind) {
    if state.active_app.get().is_some() {
        state
            .status
            .set_text("Close the active entertainment app before launching another");
        return;
    }
    let (command, workspace, label) = {
        let session_guard = state.session.borrow();
        let Some(session) = session_guard.as_ref() else {
            return;
        };
        let (profile, command, label) = match kind {
            AppKind::Steam => (
                Profile::Gaming,
                session.config().apps.steam.clone(),
                "Steam • 1080p 120 Hz • performance",
            ),
            AppKind::Kodi => (
                Profile::Media,
                session.config().apps.kodi.clone(),
                "Kodi • 4K 60 Hz • balanced",
            ),
        };
        if let Err(error) = session.apply_profile(profile) {
            state
                .status
                .set_text(&format!("Profile switch failed: {error}"));
            return;
        }
        (command, session.config().workspace.clone(), label)
    };
    match spawn_command(&command) {
        Ok(child) => {
            state.active_app.set(Some(kind));
            state.status.set_text(label);
            state.window.set_visible(false);
            match kind {
                AppKind::Kodi => spawn_child_waiter(child, kind, state.events.clone()),
                AppKind::Steam => {
                    drop(child);
                    spawn_steam_monitor(workspace, state.events.clone());
                }
            }
        }
        Err(error) => {
            if let Some(session) = state.session.borrow().as_ref() {
                let _ = session.apply_profile(Profile::Home);
            }
            state.status.set_text(&format!("Launch failed: {error}"));
        }
    }
}

fn request_exit(state: &Rc<UiState>) {
    if state.active_app.get().is_some() || app_on_tv_workspace(state) {
        state
            .status
            .set_text("Exit is locked while an entertainment app is active");
        return;
    }
    state.app.quit();
}

fn update_hold(state: &Rc<UiState>) {
    let Some((tile, started)) = *state.hold.borrow() else {
        return;
    };
    if state.hold_completed.get() || started.elapsed() < POWER_HOLD {
        return;
    }
    state.hold_completed.set(true);
    match tile {
        Tile::Sleep => {
            state.status.set_text("Locking and suspending…");
            state.window.set_visible(false);
            if let Err(error) = lock_and_suspend() {
                state.window.present();
                state.status.set_text(&format!("Suspend failed: {error}"));
            }
        }
        Tile::Shutdown => {
            state.status.set_text("Shutting down…");
            if let Err(error) = Command::new("systemctl").arg("poweroff").spawn() {
                state.status.set_text(&format!("Shutdown failed: {error}"));
            }
        }
        _ => {}
    }
}

fn lock_and_suspend() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory is unavailable"))?;
    let lock = home.join(".dotfiles/scripts/session_lock.sh");
    let status = Command::new(&lock)
        .status()
        .with_context(|| format!("failed to run {}", lock.display()))?;
    if !status.success() {
        bail!("session lock exited with {status}");
    }
    Command::new("systemctl")
        .arg("suspend")
        .spawn()
        .context("failed to start suspend")?;
    Ok(())
}

fn spawn_command(spec: &CommandSpec) -> Result<Child> {
    Command::new(&spec.program)
        .args(&spec.args)
        .spawn()
        .with_context(|| format!("failed to launch {}", spec.program))
}

fn spawn_child_waiter(mut child: Child, kind: AppKind, sender: Sender<UiEvent>) {
    thread::spawn(move || match child.wait() {
        Ok(_) => {
            let _ = sender.send(UiEvent::AppExited(kind));
        }
        Err(error) => {
            let _ = sender.send(UiEvent::Error(format!("App monitor failed: {error}")));
            let _ = sender.send(UiEvent::AppExited(kind));
        }
    });
}

fn spawn_steam_monitor(workspace: String, sender: Sender<UiEvent>) {
    thread::spawn(move || {
        let started = Instant::now();
        let mut seen_client = false;
        let mut absent_since: Option<Instant> = None;
        loop {
            match steam_client_state(&workspace) {
                Ok(state) if state.session_active => {
                    for address in state.games_needing_fullscreen {
                        if let Err(error) = enforce_game_fullscreen(&address) {
                            eprintln!("failed to keep Steam game {address} fullscreen: {error:#}");
                        }
                    }
                    seen_client = true;
                    absent_since = None;
                }
                Ok(_) if seen_client => {
                    let since = absent_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= APP_EXIT_STABLE {
                        let _ = sender.send(UiEvent::AppExited(AppKind::Steam));
                        return;
                    }
                }
                Ok(_) if started.elapsed() >= APP_START_GRACE => {
                    let _ = sender.send(UiEvent::Error(
                        "Steam did not open a window on the TV workspace".to_string(),
                    ));
                    let _ = sender.send(UiEvent::AppExited(AppKind::Steam));
                    return;
                }
                Ok(_) => {}
                Err(_) => {
                    // A compositor query failure is not evidence that Steam exited.
                    absent_since = None;
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
}

fn app_on_tv_workspace(state: &UiState) -> bool {
    state.session.borrow().as_ref().is_some_and(|session| {
        mapped_clients_on_workspace(&session.config().workspace).unwrap_or(true)
    })
}

fn mapped_clients_on_workspace(workspace: &str) -> Result<bool> {
    let clients = hyprland_clients()?;
    Ok(has_mapped_client_on_workspace(&clients, workspace))
}

fn steam_client_state(workspace: &str) -> Result<SteamClientState> {
    let clients = hyprland_clients()?;
    Ok(steam_client_state_from_json(&clients, workspace))
}

fn hyprland_clients() -> Result<Value> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .context("failed to query Hyprland clients")?;
    if !output.status.success() {
        bail!("hyprctl clients failed");
    }
    serde_json::from_slice(&output.stdout).context("failed to parse Hyprland clients")
}

fn has_mapped_client_on_workspace(clients: &Value, workspace: &str) -> bool {
    clients.as_array().is_some_and(|clients| {
        clients.iter().any(|client| {
            client
                .get("workspace")
                .and_then(|workspace| workspace.get("name"))
                .and_then(Value::as_str)
                == Some(workspace)
                && client
                    .get("mapped")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
        })
    })
}

#[derive(Debug, Default, Eq, PartialEq)]
struct SteamClientState {
    session_active: bool,
    games_needing_fullscreen: Vec<String>,
}

fn steam_client_state_from_json(clients: &Value, workspace: &str) -> SteamClientState {
    let mut state = SteamClientState::default();
    let Some(clients) = clients.as_array() else {
        return state;
    };
    for client in clients {
        if !client
            .get("mapped")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            continue;
        }
        let on_tv_workspace = client
            .get("workspace")
            .and_then(|workspace| workspace.get("name"))
            .and_then(Value::as_str)
            == Some(workspace);
        let class = client
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let initial_class = client
            .get("initialClass")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_steam =
            class.eq_ignore_ascii_case("steam") || initial_class.eq_ignore_ascii_case("steam");
        let is_game = class.starts_with("steam_app_") || initial_class.starts_with("steam_app_");
        state.session_active |= on_tv_workspace || is_steam || is_game;
        if is_game
            && (client.get("fullscreen").and_then(Value::as_i64) != Some(2)
                || client.get("fullscreenClient").and_then(Value::as_i64) != Some(2))
            && let Some(address) = client.get("address").and_then(Value::as_str)
        {
            state.games_needing_fullscreen.push(address.to_string());
        }
    }
    state
}

fn enforce_game_fullscreen(address: &str) -> Result<()> {
    hyprland_dispatch(&format!(
        "hl.dsp.focus({{ window = {:?} }})",
        format!("address:{address}")
    ))?;
    hyprland_dispatch(
        "hl.dsp.window.fullscreen_state({ internal = 2, client = 2, action = \"set\" })",
    )
}

fn hyprland_dispatch(dispatcher: &str) -> Result<()> {
    let output = Command::new("hyprctl")
        .args(["dispatch", dispatcher])
        .output()
        .with_context(|| format!("failed to dispatch Hyprland {dispatcher}"))?;
    if !output.status.success() {
        bail!(
            "hyprctl dispatch {dispatcher} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn navigate(state: &UiState, direction: NavDirection) {
    let current = state.selected.get();
    let next = match (current, direction) {
        (0, NavDirection::Right) => 1,
        (1, NavDirection::Left) => 0,
        (0, NavDirection::Down) => 2,
        (1, NavDirection::Down) => 4,
        (2, NavDirection::Up) => 0,
        (3 | 4, NavDirection::Up) => 1,
        (2, NavDirection::Right) => 3,
        (3, NavDirection::Left) => 2,
        (3, NavDirection::Right) => 4,
        (4, NavDirection::Left) => 3,
        _ => current,
    };
    state.selected.set(next);
    state.buttons[next].grab_focus();
}

fn spawn_gamepad_worker(sender: Sender<UiEvent>, cancelled: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(gilrs) => gilrs,
            Err(error) => {
                let _ = sender.send(UiEvent::Error(format!(
                    "Controller support unavailable: {error}"
                )));
                return;
            }
        };
        for (_, gamepad) in gilrs.gamepads() {
            let _ = sender.send(UiEvent::Controller(gamepad.name().to_string()));
        }
        let mut guide_started: Option<Instant> = None;
        let mut guide_sent = false;
        let mut stick_x = 0_i8;
        let mut stick_y = 0_i8;
        while !cancelled.load(Ordering::Relaxed) {
            while let Some(event) = gilrs.next_event() {
                match event.event {
                    EventType::Connected => {
                        let name = gilrs.gamepad(event.id).name().to_string();
                        let _ = sender.send(UiEvent::Controller(name));
                    }
                    EventType::ButtonPressed(Button::DPadLeft, _) => {
                        let _ = sender.send(UiEvent::Navigate(NavDirection::Left));
                    }
                    EventType::ButtonPressed(Button::DPadRight, _) => {
                        let _ = sender.send(UiEvent::Navigate(NavDirection::Right));
                    }
                    EventType::ButtonPressed(Button::DPadUp, _) => {
                        let _ = sender.send(UiEvent::Navigate(NavDirection::Up));
                    }
                    EventType::ButtonPressed(Button::DPadDown, _) => {
                        let _ = sender.send(UiEvent::Navigate(NavDirection::Down));
                    }
                    EventType::ButtonPressed(Button::South, _) => {
                        let _ = sender.send(UiEvent::ActivatePressed);
                    }
                    EventType::ButtonReleased(Button::South, _) => {
                        let _ = sender.send(UiEvent::ActivateReleased);
                    }
                    EventType::ButtonPressed(Button::East, _) => {
                        let _ = sender.send(UiEvent::Back);
                    }
                    EventType::ButtonPressed(Button::Mode, _) => {
                        guide_started = Some(Instant::now());
                        guide_sent = false;
                    }
                    EventType::ButtonReleased(Button::Mode, _) => {
                        guide_started = None;
                        guide_sent = false;
                    }
                    EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                        let next = axis_bucket(value);
                        if next != stick_x && next != 0 {
                            let _ = sender.send(UiEvent::Navigate(if next < 0 {
                                NavDirection::Left
                            } else {
                                NavDirection::Right
                            }));
                        }
                        stick_x = next;
                    }
                    EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                        let next = axis_bucket(value);
                        if next != stick_y && next != 0 {
                            let _ = sender.send(UiEvent::Navigate(if next < 0 {
                                NavDirection::Down
                            } else {
                                NavDirection::Up
                            }));
                        }
                        stick_y = next;
                    }
                    _ => {}
                }
            }
            if !guide_sent && guide_started.is_some_and(|started| started.elapsed() >= GUIDE_HOLD) {
                guide_sent = true;
                let _ = sender.send(UiEvent::Show);
            }
            thread::sleep(Duration::from_millis(10));
        }
    })
}

fn axis_bucket(value: f32) -> i8 {
    if value <= -0.65 {
        -1
    } else if value >= 0.65 {
        1
    } else {
        0
    }
}

fn monitor_for_connector(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    display
        .monitors()
        .snapshot()
        .into_iter()
        .filter_map(|item| item.downcast::<gdk::Monitor>().ok())
        .find(|monitor| monitor.connector().as_deref() == Some(connector))
}

fn tile_button(icon: &str, title: &str, subtitle: &str, classes: &str) -> gtk::Button {
    let button = gtk::Button::new();
    for class in classes.split_whitespace() {
        button.add_css_class(class);
    }
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_halign(gtk::Align::Start);
    content.set_valign(gtk::Align::Center);
    let icon = gtk::Label::new(Some(icon));
    icon.add_css_class("tile-icon");
    icon.set_xalign(0.0);
    let title = gtk::Label::new(Some(title));
    title.add_css_class("tile-title");
    title.set_xalign(0.0);
    let subtitle = gtk::Label::new(Some(subtitle));
    subtitle.add_css_class("tile-subtitle");
    subtitle.set_xalign(0.0);
    content.append(&icon);
    content.append(&title);
    content.append(&subtitle);
    button.set_child(Some(&content));
    button
}

fn load_css() -> Result<()> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tv-mode");
    let provider = gtk::CssProvider::new();
    let mut css = std::fs::read_to_string(config_dir.join("style.css"))
        .context("failed to read TV mode style.css")?;
    if let Ok(colors) = std::fs::read_to_string(config_dir.join("colors.css")) {
        css.push('\n');
        css.push_str(&colors);
    }
    provider.load_from_string(&css);
    let display = gdk::Display::default().ok_or_else(|| anyhow!("GDK display is unavailable"))?;
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Ok(())
}

fn update_clock(label: &gtk::Label) {
    let text = glib::DateTime::now_local()
        .ok()
        .and_then(|time| time.format("%H:%M").ok())
        .map(|value| value.to_string())
        .unwrap_or_default();
    label.set_text(&text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_threshold_requires_release_before_repeating() {
        assert_eq!(axis_bucket(-1.0), -1);
        assert_eq!(axis_bucket(1.0), 1);
        assert_eq!(axis_bucket(0.0), 0);
        assert_eq!(axis_bucket(0.5), 0);
    }

    #[test]
    fn power_tiles_are_hold_only() {
        assert!(Tile::Sleep.is_power());
        assert!(Tile::Shutdown.is_power());
        assert!(!Tile::Exit.is_power());
    }

    #[test]
    fn borderless_game_on_another_workspace_stays_active_and_requests_fullscreen() {
        let clients = serde_json::json!([{
            "address": "0xgame",
            "class": "steam_app_252950",
            "initialClass": "steam_app_252950",
            "workspace": { "name": "3" },
            "mapped": true,
            "fullscreen": 1,
            "fullscreenClient": 1
        }]);

        assert_eq!(
            steam_client_state_from_json(&clients, "tv"),
            SteamClientState {
                session_active: true,
                games_needing_fullscreen: vec!["0xgame".to_string()],
            }
        );
    }

    #[test]
    fn true_fullscreen_game_needs_no_correction() {
        let clients = serde_json::json!([{
            "address": "0xgame",
            "class": "steam_app_252950",
            "workspace": { "name": "3" },
            "mapped": true,
            "fullscreen": 2,
            "fullscreenClient": 2
        }]);

        assert_eq!(
            steam_client_state_from_json(&clients, "tv"),
            SteamClientState {
                session_active: true,
                games_needing_fullscreen: Vec::new(),
            }
        );
    }

    #[test]
    fn steam_window_on_another_workspace_keeps_session_active() {
        let clients = serde_json::json!([{
            "address": "0xsteam",
            "class": "steam",
            "initialClass": "steam",
            "title": "Steam Big Picture Mode",
            "workspace": { "name": "5" },
            "mapped": true,
            "fullscreen": 2,
            "fullscreenClient": 2
        }]);

        assert_eq!(
            steam_client_state_from_json(&clients, "tv"),
            SteamClientState {
                session_active: true,
                games_needing_fullscreen: Vec::new(),
            }
        );
    }

    #[test]
    fn unmapped_or_other_workspace_clients_do_not_keep_session_active() {
        let clients = serde_json::json!([
            { "workspace": { "name": "tv" }, "mapped": false },
            { "workspace": { "name": "3" }, "mapped": true }
        ]);

        assert!(!has_mapped_client_on_workspace(&clients, "tv"));
    }
}
