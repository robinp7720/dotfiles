//! Qt Quick shell. Rust owns session state, credentials, IO and playback.
use crate::{
    config::Config,
    control::{self, ControlCommand},
    library::Library,
    oblecto::{self, Media},
    platform::{self, AppKind, NavDirection, UiEvent},
    player::{self, CommandKind},
    session::{Profile, Session},
};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn tv_quick_run(
        context: *mut c_void,
        poll: extern "C" fn(*mut c_void) -> *const c_char,
        action: extern "C" fn(*mut c_void, *const c_char),
        output: *const c_char,
        preview: bool,
    ) -> i32;
}
struct Shell {
    config: Config,
    session: Option<Session>,
    preview: bool,
    library: Library,
    events: Receiver<UiEvent>,
    sender: mpsc::Sender<UiEvent>,
    control: Receiver<ControlCommand>,
    player: Option<(player::Handle, Receiver<player::Event>, Media)>,
    active_app: Option<AppKind>,
    state: Value,
    dirty: bool,
    inputs: Vec<String>,
    window: String,
    visible: bool,
    quit: bool,
    buffer: CString,
    hold: Option<(String, Instant)>,
    last_interaction: Instant,
    focus_at: Option<Instant>,
}
pub fn run(session: Session) -> Result<()> {
    run_shell(session.config().clone(), Some(session), false, true)
}
pub fn preview(config: Config, controllers: bool) -> Result<()> {
    run_shell(config, None, true, controllers)
}
fn run_shell(
    config: Config,
    session: Option<Session>,
    preview: bool,
    controllers: bool,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let (ctx, crx) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let library = Library::new(&config.oblecto)?;
    let control = if preview {
        None
    } else {
        Some(control::spawn_server(ctx, Arc::clone(&cancelled))?)
    };
    let gamepad =
        controllers.then(|| platform::spawn_gamepad_worker(tx.clone(), Arc::clone(&cancelled)));
    let output = CString::new(config.output.clone())?;
    let display_mode = crate::config::mode_label(&config.modes.home);
    let mut shell = Shell {
        config,
        session,
        preview,
        library,
        events: rx,
        sender: tx,
        control: crx,
        player: None,
        active_app: None,
        state: json!({"displayMode":display_mode,"preview":preview,"controller":"Keyboard & remote","playing":false,"activeApp":"","holdProgress":0,"theme":theme(),"reducedMotion":std::env::var("TV_MODE_REDUCED_MOTION").is_ok_and(|v|v=="1")}),
        dirty: true,
        inputs: vec![],
        window: String::new(),
        visible: true,
        quit: false,
        buffer: CString::default(),
        hold: None,
        last_interaction: Instant::now(),
        focus_at: None,
    };
    // The C++ host invokes callbacks synchronously on this thread, never after returning.
    let code = unsafe {
        tv_quick_run(
            (&mut shell as *mut Shell).cast(),
            poll,
            action,
            output.as_ptr(),
            preview,
        )
    };
    cancelled.store(true, Ordering::Relaxed);
    if let Some((handle, _, _)) = shell.player.take() {
        handle.send(CommandKind::Stop);
        handle.finish();
    }
    if let Some(control) = control {
        let _ = control.join();
    }
    if let Some(gamepad) = gamepad {
        let _ = gamepad.join();
    }
    if let Some(mut session) = shell.session.take() {
        session.restore()?;
    }
    if code != 0 {
        bail!("Qt Quick interface exited with code {code}");
    }
    Ok(())
}
extern "C" fn poll(context: *mut c_void) -> *const c_char {
    // No Rust unwind may cross the C ABI; leave the event loop for normal cleanup.
    let shell = unsafe { &mut *context.cast::<Shell>() };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shell.tick())).is_err() {
        shell.quit = true;
    }
    let mut value = json!({"inputs":std::mem::take(&mut shell.inputs),"window":std::mem::take(&mut shell.window),"quit":shell.quit});
    if shell.dirty || shell.library.dirty {
        shell.state["library"] = shell.library.state.clone();
        value["state"] = shell.state.clone();
        shell.dirty = false;
        shell.library.dirty = false;
    }
    shell.buffer = CString::new(value.to_string()).unwrap_or_default();
    shell.buffer.as_ptr()
}
extern "C" fn action(context: *mut c_void, bytes: *const c_char) {
    let shell = unsafe { &mut *context.cast::<Shell>() };
    let bytes = unsafe { CStr::from_ptr(bytes) };
    if let Ok(value) = serde_json::from_slice(bytes.to_bytes())
        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shell.action(value))).is_err()
    {
        shell.quit = true;
    }
}
impl Shell {
    fn tick(&mut self) {
        self.library.poll();
        while let Ok(command) = self.control.try_recv() {
            match command {
                ControlCommand::Show => self.show_home(),
                ControlCommand::Exit => self.exit(),
            }
        }
        while let Ok(event) = self.events.try_recv() {
            self.event(event);
        }
        let mut playback = vec![];
        if let Some((_, rx, _)) = &self.player {
            while let Ok(e) = rx.try_recv() {
                playback.push(e);
            }
        }
        for event in playback {
            match event {
                player::Event::Ready => {
                    self.visibility(false);
                    self.library.state["loading"] = false.into();
                    self.library.message("");
                }
                player::Event::Interaction => self.last_interaction = Instant::now(),
                player::Event::Notice(message) => self.library.message(message),
                player::Event::Finished { ended, error } => {
                    if let Some((handle, _, item)) = self.player.take() {
                        handle.finish();
                        self.state["playing"] = false.into();
                        self.dirty = true;
                        self.visibility(true);
                        self.profile(Profile::Home);
                        let prompt = self.library.me["preferences"]["stillWatchingPrompt"] != false
                            && self.last_interaction.elapsed() >= Duration::from_secs(5400);
                        self.library.playback_finished(&item, ended, prompt);
                        if let Some(e) = error {
                            self.library.message(e);
                        }
                    }
                }
            }
        }
        if let Some(request) = self.library.play_request.take() {
            self.play(request);
        }
        if let Some((kind, start)) = &self.hold {
            let progress = (start.elapsed().as_secs_f64() / 2.0).min(1.0);
            self.state["holdProgress"] = progress.into();
            self.dirty = true;
            if progress >= 1.0 {
                let kind = kind.clone();
                self.hold = None;
                self.power(&kind);
            }
        }
        if self.focus_at.is_some_and(|t| Instant::now() >= t) {
            self.focus_at = None;
            if !self.preview {
                let _ = crate::session::hyprland_dispatch(
                    "hl.dsp.focus({ window = \"class:^me.robindecker.TvMode$\" })",
                );
            }
        }
    }
    fn action(&mut self, a: Value) {
        self.last_interaction = Instant::now();
        match a["type"].as_str().unwrap_or("") {
            "app" => self.launch(if a["name"] == "steam" {
                AppKind::Steam
            } else {
                AppKind::Kodi
            }),
            "exit" => self.exit(),
            "home" => self.show_home(),
            "resumeApp" => {
                if self.player.is_some() || self.active_app.is_some() {
                    self.visibility(false);
                }
            }
            "powerPress" => {
                if let Some(kind) = a["name"]
                    .as_str()
                    .filter(|s| ["sleep", "shutdown"].contains(s))
                {
                    self.hold = Some((kind.into(), Instant::now()));
                }
            }
            "powerRelease" => {
                self.hold = None;
                self.state["holdProgress"] = 0.into();
                self.dirty = true;
            }
            "motion" => {
                self.state["reducedMotion"] = a["reduced"].clone();
                self.dirty = true;
            }
            "back" if self.player.is_some() && self.library.state["page"] == "home" => {
                self.visibility(false)
            }
            "back" if self.player.is_some() => {
                self.player.as_ref().unwrap().0.send(CommandKind::Stop);
            }
            "back" if self.library.state["page"] == "home" && self.active_app.is_some() => {
                self.visibility(false)
            }
            "back" if self.library.state["page"] == "home" => self.exit(),
            "play" if self.active_app.is_some() || self.player.is_some() => self
                .library
                .message("Close the active app before starting another title."),
            "logout" if self.player.is_some() => {
                self.library.message("Stop playback before signing out.")
            }
            _ => self.library.action(a),
        }
    }
    fn visibility(&mut self, show: bool) {
        self.visible = show;
        self.window = if show { "show" } else { "hide" }.into();
        if show {
            self.focus_at = Some(Instant::now() + Duration::from_millis(120));
        }
    }
    fn profile(&mut self, profile: Profile) {
        if let Some(session) = &self.session
            && let Err(e) = session.apply_profile(profile)
        {
            self.library.message(format!("Display profile: {e}"));
        }
    }
    fn show_home(&mut self) {
        self.hold = None;
        self.library.action(json!({"type":"home"}));
        self.visibility(true);
        if self.player.is_none() && self.active_app.is_none() {
            self.profile(Profile::Home);
        }
    }
    fn event(&mut self, event: UiEvent) {
        if let Some((handle, _, _)) = &self.player
            && !self.visible
        {
            let command = match event {
                UiEvent::Navigate(NavDirection::Left) => Some(CommandKind::Seek(-10.0)),
                UiEvent::Navigate(NavDirection::Right) => Some(CommandKind::Seek(10.0)),
                UiEvent::Navigate(NavDirection::Up) => Some(CommandKind::Volume(5.0)),
                UiEvent::Navigate(NavDirection::Down) => Some(CommandKind::Volume(-5.0)),
                UiEvent::ActivatePressed => Some(CommandKind::TogglePause),
                UiEvent::Back => Some(CommandKind::Stop),
                UiEvent::PlayerControl(command) => Some(command),
                _ => None,
            };
            if let Some(command) = command {
                self.last_interaction = Instant::now();
                handle.send(command);
                return;
            }
        }
        match event {
            UiEvent::Show => self.show_home(),
            UiEvent::ExitRequested if self.visible || self.player.is_some() => self.exit(),
            UiEvent::Navigate(dir) if self.visible => self.inputs.push(
                match dir {
                    NavDirection::Left => "left",
                    NavDirection::Right => "right",
                    NavDirection::Up => "up",
                    NavDirection::Down => "down",
                }
                .into(),
            ),
            UiEvent::ActivatePressed if self.visible => self.inputs.push("press".into()),
            UiEvent::ActivateReleased => self.inputs.push("release".into()),
            UiEvent::Back if self.visible => self.inputs.push("back".into()),
            UiEvent::Controller(name) => {
                self.state["controller"] = name.into();
                self.dirty = true;
            }
            UiEvent::Error(message) => self.library.message(message),
            UiEvent::AppExited(kind) if self.active_app == Some(kind) => {
                self.active_app = None;
                self.state["activeApp"] = "".into();
                self.dirty = true;
                self.profile(Profile::Home);
                self.visibility(true);
            }
            _ => {}
        }
    }
    fn play(&mut self, (item, file, position): (Media, Value, f64)) {
        if self.player.is_some() || self.active_app.is_some() {
            return;
        }
        let Some(api) = self.library.api.clone() else {
            return;
        };
        self.profile(Profile::Media);
        let options = oblecto::playback_options(&file, &self.library.me["preferences"], position);
        let (handle, events) = player::start(
            api,
            options,
            self.config.oblecto.player.clone(),
            !self.preview,
        );
        self.player = Some((handle, events, item));
        self.state["playing"] = true.into();
        self.library.message("Preparing your film…  B to cancel");
        self.dirty = true;
    }
    fn launch(&mut self, kind: AppKind) {
        if self.preview {
            self.library
                .message("App launching is disabled in preview.");
            return;
        }
        if self.active_app.is_some() || self.player.is_some() {
            self.library
                .message("Close the active app before launching another.");
            return;
        }
        let (profile, command) = match kind {
            AppKind::Steam => (Profile::Gaming, &self.config.apps.steam),
            AppKind::Kodi => (Profile::Media, &self.config.apps.kodi),
        };
        if let Some(session) = &self.session
            && let Err(e) = session.apply_profile(profile)
        {
            self.library.message(e.to_string());
            return;
        }
        match platform::spawn_command(command) {
            Ok(child) => {
                self.active_app = Some(kind);
                self.state["activeApp"] = format!("{kind:?}").into();
                self.dirty = true;
                self.visibility(false);
                match kind {
                    AppKind::Steam => platform::spawn_steam_monitor(
                        child,
                        self.config.workspace.clone(),
                        self.sender.clone(),
                    ),
                    AppKind::Kodi => platform::spawn_child_waiter(child, kind, self.sender.clone()),
                }
            }
            Err(e) => {
                self.profile(Profile::Home);
                self.library.message(e.to_string());
            }
        }
    }
    fn exit(&mut self) {
        if self.active_app.is_some()
            || (!self.preview
                && self.player.is_none()
                && platform::mapped_clients_on_workspace(&self.config.workspace).unwrap_or(true))
        {
            self.library
                .message("Close the active entertainment app before leaving TV mode.");
            self.visibility(true);
        } else {
            self.quit = true;
        }
    }
    fn power(&mut self, kind: &str) {
        if self.preview {
            self.library
                .message("Power actions are disabled in preview.");
            return;
        }
        let result = if kind == "sleep" {
            platform::lock_and_suspend()
        } else {
            std::process::Command::new("systemctl")
                .arg("poweroff")
                .spawn()
                .map(|_| ())
                .map_err(Into::into)
        };
        if let Err(e) = result {
            self.library.message(e.to_string());
        }
    }
}
fn theme() -> Value {
    let mut colors = json!({"primary":"#83d2e3","on_primary":"#00363e","tv_text":"#dee3e5","tv_muted":"#bfc8cb","tv_background":"#0e1416","tv_surface":"#1b2122","tertiary":"#bcc5eb"});
    if let Some(path) = dirs::config_dir().map(|d| d.join("tv-mode/colors.css"))
        && let Ok(text) = std::fs::read_to_string(path)
    {
        for line in text.lines() {
            let words: Vec<_> = line.split_whitespace().collect();
            if words.len() == 3 && words[0] == "@define-color" && words[2].starts_with('#') {
                colors[words[1]] = words[2].trim_end_matches(';').into();
            }
        }
    }
    colors
}
