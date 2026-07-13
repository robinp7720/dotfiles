pub mod actions;
pub mod activity;
pub mod compositor;
pub mod config;
pub mod context;
pub mod ipc;
pub mod model;
pub mod sources;
pub mod state;
pub mod timers;
pub mod ui;

use std::time::{SystemTime, UNIX_EPOCH};

use std::path::Path;

use anyhow::Result;

pub use actions::{
    ActionBackend, ActionCompletion, ActionRequest, ActionResult, ActionRouter, ProcessSpec,
    SystemActionBackend, spawn_action_worker,
};
pub use activity::ActivityTracker;
pub use compositor::{
    CompositorAction, CompositorAdapter, HyprlandAdapter, NiriAdapter, detect_compositor,
};
pub use config::{
    AppConfig, CommandActivityConfig, CommandRule, FreshnessConfig, ModuleConfig, ModuleName,
    ThresholdConfig,
};
pub use context::{ContextCard, ContextTier, Dismissals, select_context};
pub use ipc::{ControlClient, ControlRequest, ControlResponse, ControlSocket, control_socket_path};
pub use model::{
    ActionIntent, ActivityState, ActivityStatus, ActivityUpdate, AudioState, BarSnapshot,
    BluetoothState, CalendarEvent, ClockState, CommandActivity, ConnectivityState, Direction,
    MediaControlAction, MediaState, NetworkState, OutputRole, OutputState, PlaybackStatus,
    PowerProfile, PowerState, ResourceState, SourceHealth, SourceId, StateUpdate, SystemState,
    SystemUpdate, TimerState, WindowState, WorkspaceState,
};
pub use sources::{
    CalendarRecord, SourceSupervisor, battery_severity, parse_calendar_json, read_proc_sample,
    spawn_audio_source, spawn_bluetooth_source, spawn_calendar_source, spawn_clock_source,
    spawn_media_source, spawn_network_source, spawn_power_source, spawn_resource_source,
};
pub use state::StateStore;
pub use timers::{TimerRecord, TimerStore};
pub use ui::BarApplication;

pub fn startup(_config: &AppConfig) -> Result<()> {
    Ok(())
}

pub fn run_test_control_server(requests: usize) -> Result<()> {
    let socket = ControlSocket::bind()?;
    let mut store = TimerStore::load(current_epoch())?;

    for _ in 0..requests {
        socket.serve_once(|request| match request {
            ControlRequest::TimerStart { .. }
            | ControlRequest::TimerPause { .. }
            | ControlRequest::TimerResume { .. }
            | ControlRequest::TimerCancel { .. }
            | ControlRequest::TimerList => Ok(ControlResponse::Timers {
                timers: store.apply(&request, current_epoch())?,
            }),
            ControlRequest::ActivityStart { .. } | ControlRequest::ActivityFinish { .. } => {
                Ok(ControlResponse::Accepted)
            }
        })?;
    }

    Ok(())
}

pub fn run(config_path: &Path) -> Result<()> {
    let config = AppConfig::load(config_path)?;
    startup(&config)?;
    BarApplication::new(config, config_path)?.run();
    Ok(())
}

fn current_epoch() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time");
    i64::try_from(now.as_secs()).unwrap_or(i64::MAX)
}
