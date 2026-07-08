pub mod config;
pub mod model;

use std::path::Path;

use anyhow::Result;

pub use config::{
    AppConfig, CommandActivityConfig, CommandRule, ModuleConfig, ModuleName, ThresholdConfig,
};
pub use model::{
    ActionIntent, ActivityState, ActivityStatus, ActivityUpdate, BarSnapshot, BluetoothState,
    CalendarEvent, ClockState, CommandActivity, ConnectivityState, Direction, MediaControlAction,
    MediaState, NetworkState, OutputRole, OutputState, PlaybackStatus, PowerProfile, PowerState,
    ResourceState, SourceHealth, SourceId, StateUpdate, SystemState, SystemUpdate, TimerState,
    WindowState, WorkspaceState,
};

pub fn startup(_config: &AppConfig) -> Result<()> {
    Ok(())
}

pub fn run(config_path: &Path) -> Result<()> {
    let config = AppConfig::load(config_path)?;
    startup(&config)
}
