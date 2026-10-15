//! Controller input and entertainment process monitoring, independent of the renderer.
use crate::{config::CommandSpec, player::CommandKind, session::hyprland_dispatch};
use anyhow::{Context, Result, anyhow, bail};
use gilrs::{Axis, Button, EventType, Gilrs};
use serde_json::Value;
use std::process::{Child, Command};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
const GUIDE_HOLD: Duration = Duration::from_millis(1500);
const APP_START_GRACE: Duration = Duration::from_secs(30);
const APP_EXIT_STABLE: Duration = Duration::from_secs(15);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppKind {
    Steam,
    Kodi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone)]
pub enum UiEvent {
    Navigate(NavDirection),
    ActivatePressed,
    ActivateReleased,
    Back,
    Show,
    ExitRequested,
    AppExited(AppKind),
    PlayerControl(CommandKind),
    Controller(String),
    Error(String),
}

pub fn lock_and_suspend() -> Result<()> {
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

pub fn spawn_command(spec: &CommandSpec) -> Result<Child> {
    Command::new(&spec.program)
        .args(&spec.args)
        .spawn()
        .with_context(|| format!("failed to launch {}", spec.program))
}

pub fn spawn_child_waiter(mut child: Child, kind: AppKind, sender: Sender<UiEvent>) {
    thread::spawn(move || {
        let mut seen_window = false;
        let mut absent_since = None;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = sender.send(UiEvent::AppExited(kind));
                    if !status.success() {
                        let _ =
                            sender.send(UiEvent::Error(format!("{kind:?} exited with {status}")));
                    }
                    return;
                }
                Err(error) => {
                    let _ = sender.send(UiEvent::AppExited(kind));
                    let _ = sender.send(UiEvent::Error(format!("App monitor failed: {error}")));
                    return;
                }
                Ok(None) => {}
            }
            match hyprland_clients() {
                Ok(clients) if has_app_window(&clients, kind) => {
                    seen_window = true;
                    absent_since = None;
                }
                Ok(_) if seen_window => {
                    let since = absent_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= APP_EXIT_STABLE {
                        // Kodi's shell wrapper can spend minutes generating a crash
                        // report after the window is gone. Restore home while it
                        // finishes, and reap it without emitting a second exit event.
                        let _ = sender.send(UiEvent::AppExited(kind));
                        let _ = child.wait();
                        return;
                    }
                }
                Ok(_) => {}
                Err(_) => absent_since = None,
            }
            thread::sleep(Duration::from_millis(250));
        }
    });
}

fn has_app_window(clients: &Value, kind: AppKind) -> bool {
    let class = match kind {
        AppKind::Kodi => "kodi",
        AppKind::Steam => "steam",
    };
    clients.as_array().is_some_and(|clients| {
        clients.iter().any(|client| {
            client
                .get("mapped")
                .and_then(Value::as_bool)
                .unwrap_or(true)
                && ["class", "initialClass"].iter().any(|key| {
                    client
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case(class))
                })
        })
    })
}

pub fn spawn_steam_monitor(mut child: Child, workspace: String, sender: Sender<UiEvent>) {
    let (launcher_tx, launcher_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = launcher_tx.send(child.wait());
    });
    thread::spawn(move || {
        let mut launcher_finished_at: Option<Instant> = None;
        let mut seen_ready_client = false;
        let mut absent_since: Option<Instant> = None;
        loop {
            if launcher_finished_at.is_none() && launcher_rx.try_recv().is_ok() {
                launcher_finished_at = Some(Instant::now());
            }
            match steam_client_state(&workspace) {
                Ok(state) if state.session_active => {
                    for address in state.games_needing_fullscreen {
                        if let Err(error) = enforce_game_fullscreen(&address) {
                            eprintln!("failed to keep Steam game {address} fullscreen: {error:#}");
                        }
                    }
                    seen_ready_client |= state.session_ready;
                    absent_since = None;
                }
                Ok(_) if seen_ready_client => {
                    let since = absent_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= APP_EXIT_STABLE {
                        let _ = sender.send(UiEvent::AppExited(AppKind::Steam));
                        return;
                    }
                }
                // Steam can spend longer than the grace period updating before
                // Big Picture appears. Only time out after its launcher exits.
                Ok(_)
                    if launcher_finished_at
                        .is_some_and(|finished| finished.elapsed() >= APP_START_GRACE) =>
                {
                    let _ = sender.send(UiEvent::AppExited(AppKind::Steam));
                    let _ = sender.send(UiEvent::Error(
                        "Steam exited without opening an entertainment window".to_string(),
                    ));
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

pub fn mapped_clients_on_workspace(workspace: &str) -> Result<bool> {
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
            client.get("class").and_then(Value::as_str) != Some("me.robindecker.TvMode")
                && client
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
    session_ready: bool,
    games_needing_fullscreen: Vec<String>,
}

fn steam_client_state_from_json(clients: &Value, workspace: &str) -> SteamClientState {
    let mut state = SteamClientState::default();
    let Some(clients) = clients.as_array() else {
        return state;
    };
    for client in clients {
        if client.get("class").and_then(Value::as_str) == Some("me.robindecker.TvMode") {
            continue;
        }
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
        // Update/login windows may close well before Big Picture is ready.
        state.session_ready |=
            is_game || (is_steam && client.get("fullscreen").and_then(Value::as_i64) == Some(2));
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

pub fn spawn_gamepad_worker(sender: Sender<UiEvent>, cancelled: Arc<AtomicBool>) -> JoinHandle<()> {
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
                    EventType::ButtonPressed(Button::West, _) => {
                        let _ = sender.send(UiEvent::PlayerControl(CommandKind::Subtitles));
                    }
                    EventType::ButtonPressed(Button::North, _) => {
                        let _ = sender.send(UiEvent::PlayerControl(CommandKind::Audio));
                    }
                    EventType::ButtonPressed(Button::LeftTrigger, _) => {
                        let _ = sender.send(UiEvent::PlayerControl(CommandKind::Quality));
                    }
                    EventType::ButtonPressed(Button::RightTrigger, _) => {
                        let _ = sender.send(UiEvent::PlayerControl(CommandKind::Skip));
                    }
                    EventType::ButtonPressed(Button::Select, _) => {
                        let _ = sender.send(UiEvent::ExitRequested);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_home_is_not_an_entertainment_client() {
        let clients = serde_json::json!([{"class":"me.robindecker.TvMode", "workspace":{"name":"tv"}, "mapped":true, "fullscreen":2}]);
        assert!(!has_mapped_client_on_workspace(&clients, "tv"));
        assert!(!steam_client_state_from_json(&clients, "tv").session_active);
    }

    #[test]
    fn kodi_window_monitor_tracks_visible_app_across_workspaces() {
        let mut clients = serde_json::json!([{
            "class": "Kodi", "workspace": { "name": "5" }, "mapped": true
        }]);
        assert!(has_app_window(&clients, AppKind::Kodi));
        clients[0]["mapped"] = Value::Bool(false);
        assert!(!has_app_window(&clients, AppKind::Kodi));
        assert!(!has_app_window(&serde_json::json!([]), AppKind::Kodi));
    }

    #[test]
    fn steam_updater_is_not_a_ready_entertainment_session() {
        let clients = serde_json::json!([{
            "class": "steam", "title": "Updating Steam",
            "workspace": { "name": "tv" }, "mapped": true, "fullscreen": 0
        }]);
        let state = steam_client_state_from_json(&clients, "tv");
        assert!(state.session_active);
        assert!(!state.session_ready);
    }

    #[test]
    fn stick_threshold_requires_release_before_repeating() {
        assert_eq!(axis_bucket(-1.0), -1);
        assert_eq!(axis_bucket(1.0), 1);
        assert_eq!(axis_bucket(0.0), 0);
        assert_eq!(axis_bucket(0.5), 0);
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
                session_ready: true,
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
                session_ready: true,
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
                session_ready: true,
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
