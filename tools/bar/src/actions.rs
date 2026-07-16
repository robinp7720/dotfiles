use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::{
    ActionIntent, CompositorAction, CompositorAdapter, ControlClient, ControlRequest,
    ControlResponse, Direction, MediaControlAction, PowerProfile, detect_compositor,
};

pub trait ActionBackend: Send {
    fn execute_compositor(&mut self, action: CompositorAction) -> Result<()>;
    fn execute_service_command(&mut self, spec: ProcessSpec) -> Result<()>;
    fn launch_process(&mut self, spec: ProcessSpec) -> Result<()>;
    fn control_timer(&mut self, request: ControlRequest) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl ProcessSpec {
    pub fn new<I, S>(program: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionResult {
    Completed,
    Failed { summary: String, detail: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionCompletion {
    pub origin: String,
    pub intent: ActionIntent,
    pub result: ActionResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRequest {
    pub origin: String,
    pub intent: ActionIntent,
}

pub struct ActionRouter<B> {
    backend: B,
    power_profile: PowerProfile,
}

impl<B> ActionRouter<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            power_profile: PowerProfile::Balanced,
        }
    }

    pub fn with_power_profile_state(mut self, power_profile: PowerProfile) -> Self {
        self.power_profile = power_profile;
        self
    }
}

impl<B: ActionBackend> ActionRouter<B> {
    pub fn execute(&mut self, intent: ActionIntent) -> ActionResult {
        match self.route(intent) {
            Ok(()) => ActionResult::Completed,
            Err(error) => ActionResult::Failed {
                summary: "Action failed".to_string(),
                detail: error.to_string(),
            },
        }
    }

    fn route(&mut self, intent: ActionIntent) -> Result<()> {
        match intent {
            ActionIntent::SwitchWorkspace { output, workspace } => self
                .backend
                .execute_compositor(CompositorAction::SwitchWorkspace { output, workspace }),
            ActionIntent::CycleWorkspace { output, direction } => self
                .backend
                .execute_compositor(CompositorAction::CycleWorkspace { output, direction }),
            ActionIntent::FocusWindow { output, window_id } => self
                .backend
                .execute_compositor(CompositorAction::FocusWindow { output, window_id }),
            ActionIntent::ToggleKeyboardLayout => self
                .backend
                .execute_compositor(CompositorAction::CycleKeyboardLayout),
            ActionIntent::OpenWindowSearch => {
                self.backend.launch_process(luma_query_process("windows"))
            }
            ActionIntent::OpenContextQuery { query } => {
                self.backend.launch_process(luma_query_process(&query))
            }
            ActionIntent::ControlMedia { player, action } => self
                .backend
                .execute_service_command(media_process(&player, action)),
            ActionIntent::SetVolumePercent { percent } => self
                .backend
                .execute_service_command(volume_process(percent)),
            ActionIntent::ToggleMute => self.backend.execute_service_command(mute_process()),
            ActionIntent::SetWifiEnabled { enabled } => {
                self.backend.execute_service_command(wifi_process(enabled))
            }
            ActionIntent::SetBluetoothPowered { powered } => self
                .backend
                .execute_service_command(bluetooth_power_process(powered)),
            ActionIntent::SetBrightnessPercent { device, percent } => self
                .backend
                .execute_service_command(brightness_process(&device, percent)),
            ActionIntent::CyclePowerProfile { direction } => {
                let next = cycle_power_profile(&self.power_profile, direction);
                self.backend
                    .execute_service_command(power_profile_process(&next))?;
                self.power_profile = next;
                Ok(())
            }
            ActionIntent::StartTimer {
                label,
                duration_seconds,
            } => self.backend.control_timer(ControlRequest::TimerStart {
                label,
                duration_seconds,
            }),
            ActionIntent::PauseTimer { id } => self
                .backend
                .control_timer(ControlRequest::TimerPause { id }),
            ActionIntent::ResumeTimer { id } => self
                .backend
                .control_timer(ControlRequest::TimerResume { id }),
            ActionIntent::CancelTimer { id } => self
                .backend
                .control_timer(ControlRequest::TimerCancel { id }),
        }
    }
}

pub fn spawn_action_worker<B>(
    mut router: ActionRouter<B>,
    sender: Sender<ActionCompletion>,
    cancelled: std::sync::Arc<AtomicBool>,
) -> (Sender<ActionRequest>, JoinHandle<()>)
where
    B: ActionBackend + 'static,
{
    let (request_sender, request_receiver) = mpsc::channel::<ActionRequest>();
    let handle = thread::spawn(move || {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            match request_receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(request) => {
                    let result = router.execute(request.intent.clone());
                    if sender
                        .send(ActionCompletion {
                            origin: request.origin,
                            intent: request.intent,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    (request_sender, handle)
}

pub struct SystemActionBackend {
    compositor: Box<dyn CompositorAdapter>,
    control_client: ControlClient,
}

impl SystemActionBackend {
    pub fn from_env() -> Result<Self> {
        let env = std::env::vars().collect::<Vec<_>>();
        let env_refs = env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();

        Ok(Self {
            compositor: detect_compositor(&env_refs)?,
            control_client: ControlClient::new()?,
        })
    }

    pub fn from_parts(
        compositor: Box<dyn CompositorAdapter>,
        control_client: ControlClient,
    ) -> Self {
        Self {
            compositor,
            control_client,
        }
    }
}

impl ActionBackend for SystemActionBackend {
    fn execute_compositor(&mut self, action: CompositorAction) -> Result<()> {
        self.compositor.execute(action)
    }

    fn execute_service_command(&mut self, spec: ProcessSpec) -> Result<()> {
        run_process(spec)
    }

    fn launch_process(&mut self, spec: ProcessSpec) -> Result<()> {
        spawn_process(spec)
    }

    fn control_timer(&mut self, request: ControlRequest) -> Result<()> {
        match self.control_client.send(&request)? {
            ControlResponse::Accepted | ControlResponse::Timers { .. } => Ok(()),
            ControlResponse::Error { message } => bail!(message),
        }
    }
}

fn run_process(spec: ProcessSpec) -> Result<()> {
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .with_context(|| format!("failed to execute {}", spec.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{} exited with {}", spec.display(), output.status);
    }

    bail!("{} exited with {}: {stderr}", spec.display(), output.status)
}

fn spawn_process(spec: ProcessSpec) -> Result<()> {
    Command::new(&spec.program)
        .args(&spec.args)
        .spawn()
        .with_context(|| format!("failed to spawn {}", spec.display()))?;
    Ok(())
}

fn luma_query_process(query: &str) -> ProcessSpec {
    ProcessSpec::new("Luma", ["--query", query])
}

fn media_process(player: &str, action: MediaControlAction) -> ProcessSpec {
    let verb = match action {
        MediaControlAction::Previous => "previous",
        MediaControlAction::Next => "next",
        MediaControlAction::PlayPause => "play-pause",
    };
    ProcessSpec::new(
        "playerctl",
        [format!("--player={player}"), verb.to_string()],
    )
}

fn volume_process(percent: u8) -> ProcessSpec {
    ProcessSpec::new(
        "wpctl",
        [
            "set-volume".to_string(),
            "@DEFAULT_AUDIO_SINK@".to_string(),
            format!("{}%", percent.min(100)),
        ],
    )
}

fn mute_process() -> ProcessSpec {
    ProcessSpec::new("wpctl", ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
}

fn wifi_process(enabled: bool) -> ProcessSpec {
    ProcessSpec::new("nmcli", ["radio", "wifi", on_off(enabled)])
}

fn bluetooth_power_process(powered: bool) -> ProcessSpec {
    ProcessSpec::new("bluetoothctl", ["power", on_off(powered)])
}

fn brightness_process(device: &str, percent: u8) -> ProcessSpec {
    ProcessSpec::new(
        "brightnessctl",
        [
            "--class=backlight".to_string(),
            "--device".to_string(),
            device.to_string(),
            "set".to_string(),
            format!("{}%", percent.min(100)),
        ],
    )
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

fn power_profile_process(profile: &PowerProfile) -> ProcessSpec {
    ProcessSpec::new("powerprofilesctl", ["set", power_profile_name(profile)])
}

fn power_profile_name(profile: &PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Performance => "performance",
        PowerProfile::Balanced => "balanced",
        PowerProfile::PowerSaver => "power-saver",
    }
}

fn cycle_power_profile(current: &PowerProfile, direction: Direction) -> PowerProfile {
    const ORDER: [PowerProfile; 3] = [
        PowerProfile::PowerSaver,
        PowerProfile::Balanced,
        PowerProfile::Performance,
    ];

    let index = ORDER
        .iter()
        .position(|candidate| candidate == current)
        .unwrap_or(1);
    let next_index = match direction {
        Direction::Next => (index + 1) % ORDER.len(),
        Direction::Previous => (index + ORDER.len() - 1) % ORDER.len(),
    };
    ORDER[next_index].clone()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use anyhow::{Result, anyhow};

    use crate::{
        ActionBackend, ActionCompletion, ActionIntent, ActionRequest, ActionResult, ActionRouter,
        CompositorAction, ControlRequest, Direction, MediaControlAction, PowerProfile, ProcessSpec,
        spawn_action_worker,
    };

    #[test]
    fn workspace_click_routes_to_compositor_backend() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        let result = router.execute(ActionIntent::SwitchWorkspace {
            output: "DP-5".to_string(),
            workspace: "3".to_string(),
        });

        assert_eq!(result, ActionResult::Completed);
        assert_eq!(
            state.lock().unwrap().compositor_actions,
            vec![CompositorAction::SwitchWorkspace {
                output: "DP-5".to_string(),
                workspace: "3".to_string(),
            }]
        );
    }

    #[test]
    fn media_controls_target_the_selected_player_without_shell_expansion() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        let previous = router.execute(ActionIntent::ControlMedia {
            player: "spotify".to_string(),
            action: MediaControlAction::Previous,
        });
        let next = router.execute(ActionIntent::ControlMedia {
            player: "spotify".to_string(),
            action: MediaControlAction::Next,
        });
        let play_pause = router.execute(ActionIntent::ControlMedia {
            player: "spotify".to_string(),
            action: MediaControlAction::PlayPause,
        });

        assert_eq!(previous, ActionResult::Completed);
        assert_eq!(next, ActionResult::Completed);
        assert_eq!(play_pause, ActionResult::Completed);
        let commands = state.lock().unwrap().service_commands.clone();
        assert_eq!(
            commands,
            vec![
                ProcessSpec::new("playerctl", ["--player=spotify", "previous"]),
                ProcessSpec::new("playerctl", ["--player=spotify", "next"]),
                ProcessSpec::new("playerctl", ["--player=spotify", "play-pause"]),
            ]
        );
        assert_no_shell_expansion(&commands);
    }

    #[test]
    fn power_profile_scroll_cycles_fixed_order() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()))
            .with_power_profile_state(PowerProfile::Balanced);

        let up = router.execute(ActionIntent::CyclePowerProfile {
            direction: Direction::Next,
        });
        let down = router.execute(ActionIntent::CyclePowerProfile {
            direction: Direction::Previous,
        });

        assert_eq!(up, ActionResult::Completed);
        assert_eq!(down, ActionResult::Completed);
        let commands = state.lock().unwrap().service_commands.clone();
        assert_eq!(
            commands,
            vec![
                ProcessSpec::new("powerprofilesctl", ["set", "performance"]),
                ProcessSpec::new("powerprofilesctl", ["set", "balanced"]),
            ]
        );
        assert_no_shell_expansion(&commands);
    }

    #[test]
    fn essential_controls_use_direct_argv_and_clamp_percentages() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        let intents = [
            ActionIntent::SetVolumePercent { percent: 140 },
            ActionIntent::ToggleMute,
            ActionIntent::SetWifiEnabled { enabled: false },
            ActionIntent::SetBluetoothPowered { powered: true },
            ActionIntent::SetBrightnessPercent {
                device: "intel_backlight".to_string(),
                percent: 0,
            },
        ];

        for intent in intents {
            assert_eq!(router.execute(intent), ActionResult::Completed);
        }

        let commands = state.lock().unwrap().service_commands.clone();
        assert_eq!(
            commands,
            vec![
                ProcessSpec::new("wpctl", ["set-volume", "@DEFAULT_AUDIO_SINK@", "100%"],),
                ProcessSpec::new("wpctl", ["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]),
                ProcessSpec::new("nmcli", ["radio", "wifi", "off"]),
                ProcessSpec::new("bluetoothctl", ["power", "on"]),
                ProcessSpec::new(
                    "brightnessctl",
                    [
                        "--class=backlight",
                        "--device",
                        "intel_backlight",
                        "set",
                        "0%",
                    ],
                ),
            ]
        );
        assert_no_shell_expansion(&commands);
    }

    #[test]
    fn title_secondary_click_launches_luma_windows_query() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        let result = router.execute(ActionIntent::OpenWindowSearch);

        assert_eq!(result, ActionResult::Completed);
        let processes = state.lock().unwrap().launched_processes.clone();
        assert_eq!(
            processes,
            vec![ProcessSpec::new("Luma", ["--query", "windows"])]
        );
        assert_no_shell_expansion(&processes);
    }

    #[test]
    fn context_secondary_click_launches_card_query() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        let result = router.execute(ActionIntent::OpenContextQuery {
            query: "power".to_string(),
        });

        assert_eq!(result, ActionResult::Completed);
        let processes = state.lock().unwrap().launched_processes.clone();
        assert_eq!(
            processes,
            vec![ProcessSpec::new("Luma", ["--query", "power"])]
        );
        assert_no_shell_expansion(&processes);
    }

    #[test]
    fn timer_actions_use_typed_control_requests() {
        let state = SpyState::default_shared();
        let mut router = ActionRouter::new(SpyBackend::new(state.clone()));

        assert_eq!(
            router.execute(ActionIntent::StartTimer {
                label: "Focus".to_string(),
                duration_seconds: 1500,
            }),
            ActionResult::Completed
        );
        assert_eq!(
            router.execute(ActionIntent::PauseTimer {
                id: "timer-1".to_string(),
            }),
            ActionResult::Completed
        );
        assert_eq!(
            router.execute(ActionIntent::ResumeTimer {
                id: "timer-1".to_string(),
            }),
            ActionResult::Completed
        );
        assert_eq!(
            router.execute(ActionIntent::CancelTimer {
                id: "timer-1".to_string(),
            }),
            ActionResult::Completed
        );

        assert_eq!(
            state.lock().unwrap().timer_requests,
            vec![
                ControlRequest::TimerStart {
                    label: "Focus".to_string(),
                    duration_seconds: 1500,
                },
                ControlRequest::TimerPause {
                    id: "timer-1".to_string(),
                },
                ControlRequest::TimerResume {
                    id: "timer-1".to_string(),
                },
                ControlRequest::TimerCancel {
                    id: "timer-1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn backend_failures_return_failed_result_for_popovers() {
        let state = SpyState::default_shared();
        state.lock().unwrap().launch_error = Some("launcher missing".to_string());
        let mut router = ActionRouter::new(SpyBackend::new(state));

        let result = router.execute(ActionIntent::OpenWindowSearch);

        assert_eq!(
            result,
            ActionResult::Failed {
                summary: "Action failed".to_string(),
                detail: "launcher missing".to_string(),
            }
        );
    }

    #[test]
    fn threaded_worker_preserves_power_profile_state_across_requests() {
        let state = SpyState::default_shared();
        let router = ActionRouter::new(SpyBackend::new(state.clone()))
            .with_power_profile_state(PowerProfile::Balanced);
        let (completion_tx, completion_rx) = mpsc::channel();
        let (request_tx, handle) =
            spawn_action_worker(router, completion_tx, Arc::new(AtomicBool::new(false)));

        request_tx
            .send(ActionRequest {
                origin: "power-popover-1".to_string(),
                intent: ActionIntent::CyclePowerProfile {
                    direction: Direction::Next,
                },
            })
            .unwrap();
        request_tx
            .send(ActionRequest {
                origin: "power-popover-2".to_string(),
                intent: ActionIntent::CyclePowerProfile {
                    direction: Direction::Next,
                },
            })
            .unwrap();

        let first = completion_rx.recv().unwrap();
        let second = completion_rx.recv().unwrap();
        drop(request_tx);
        handle.join().unwrap();

        assert_eq!(first.result, ActionResult::Completed);
        assert_eq!(second.result, ActionResult::Completed);
        assert_eq!(
            state.lock().unwrap().service_commands,
            vec![
                ProcessSpec::new("powerprofilesctl", ["set", "performance"]),
                ProcessSpec::new("powerprofilesctl", ["set", "power-saver"]),
            ]
        );
    }

    #[test]
    fn threaded_worker_completion_carries_caller_supplied_origin() {
        let state = SpyState::default_shared();
        let router = ActionRouter::new(SpyBackend::new(state));
        let (completion_tx, completion_rx) = mpsc::channel();
        let (request_tx, handle) =
            spawn_action_worker(router, completion_tx, Arc::new(AtomicBool::new(false)));

        request_tx
            .send(ActionRequest {
                origin: "context-popover:42".to_string(),
                intent: ActionIntent::OpenContextQuery {
                    query: "power".to_string(),
                },
            })
            .unwrap();

        let completion = completion_rx.recv().unwrap();
        drop(request_tx);
        handle.join().unwrap();

        assert_eq!(
            completion,
            ActionCompletion {
                origin: "context-popover:42".to_string(),
                intent: ActionIntent::OpenContextQuery {
                    query: "power".to_string(),
                },
                result: ActionResult::Completed,
            }
        );
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct SpyState {
        compositor_actions: Vec<CompositorAction>,
        service_commands: Vec<ProcessSpec>,
        launched_processes: Vec<ProcessSpec>,
        timer_requests: Vec<ControlRequest>,
        launch_error: Option<String>,
    }

    impl SpyState {
        fn default_shared() -> Arc<Mutex<Self>> {
            Arc::new(Mutex::new(Self::default()))
        }
    }

    struct SpyBackend {
        state: Arc<Mutex<SpyState>>,
    }

    impl SpyBackend {
        fn new(state: Arc<Mutex<SpyState>>) -> Self {
            Self { state }
        }
    }

    impl ActionBackend for SpyBackend {
        fn execute_compositor(&mut self, action: CompositorAction) -> Result<()> {
            self.state.lock().unwrap().compositor_actions.push(action);
            Ok(())
        }

        fn execute_service_command(&mut self, spec: ProcessSpec) -> Result<()> {
            self.state.lock().unwrap().service_commands.push(spec);
            Ok(())
        }

        fn launch_process(&mut self, spec: ProcessSpec) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.launched_processes.push(spec);
            if let Some(message) = state.launch_error.clone() {
                return Err(anyhow!(message));
            }
            Ok(())
        }

        fn control_timer(&mut self, request: ControlRequest) -> Result<()> {
            self.state.lock().unwrap().timer_requests.push(request);
            Ok(())
        }
    }

    fn assert_no_shell_expansion(specs: &[ProcessSpec]) {
        for spec in specs {
            assert_ne!(spec.program, "sh");
            assert_ne!(spec.program, "/bin/sh");
            assert_ne!(spec.program, "bash");
            assert_ne!(spec.program, "/bin/bash");
            assert!(!spec.args.iter().any(|arg| arg == "-c"));
        }
    }
}
