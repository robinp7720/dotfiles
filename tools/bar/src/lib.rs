pub mod config;
pub mod context;
pub mod model;
pub mod state;

use std::path::Path;

use anyhow::Result;

pub use config::{
    AppConfig, CommandActivityConfig, CommandRule, FreshnessConfig, ModuleConfig, ModuleName,
    ThresholdConfig,
};
pub use context::{ContextCard, ContextTier, Dismissals, select_context};
pub use model::{
    ActionIntent, ActivityState, ActivityStatus, ActivityUpdate, BarSnapshot, BluetoothState,
    CalendarEvent, ClockState, CommandActivity, ConnectivityState, Direction, MediaControlAction,
    MediaState, NetworkState, OutputRole, OutputState, PlaybackStatus, PowerProfile, PowerState,
    ResourceState, SourceHealth, SourceId, StateUpdate, SystemState, SystemUpdate, TimerState,
    WindowState, WorkspaceState,
};
pub use state::StateStore;

pub fn startup(_config: &AppConfig) -> Result<()> {
    Ok(())
}

pub fn run(config_path: &Path) -> Result<()> {
    let config = AppConfig::load(config_path)?;
    startup(&config)
}
