use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("{0}")]
    Validation(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub primary_output: Option<String>,
    pub thresholds: ThresholdConfig,
    pub modules: ModuleConfig,
    pub freshness: FreshnessConfig,
    pub retry_backoff: RetryBackoffConfig,
    pub command_activity: CommandActivityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            primary_output: Some("DP-5".to_string()),
            thresholds: ThresholdConfig::default(),
            modules: ModuleConfig::default(),
            freshness: FreshnessConfig::default(),
            retry_backoff: RetryBackoffConfig::default(),
            command_activity: CommandActivityConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<AppConfig> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::from_toml(&text)
    }

    pub fn from_toml(text: &str) -> Result<AppConfig> {
        let config: AppConfig = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        validate_percent(
            "thresholds.battery_low_percent",
            self.thresholds.battery_low_percent,
        )?;
        validate_percent(
            "thresholds.battery_critical_percent",
            self.thresholds.battery_critical_percent,
        )?;

        if self.thresholds.battery_critical_percent >= self.thresholds.battery_low_percent {
            return Err(ConfigError::Validation(
                "battery_critical_percent must be lower than battery_low_percent".to_string(),
            )
            .into());
        }

        validate_positive(
            "thresholds.critical_snooze_seconds",
            self.thresholds.critical_snooze_seconds,
        )?;

        validate_positive(
            "freshness.compositor_seconds",
            self.freshness.compositor_seconds,
        )?;
        validate_positive(
            "freshness.resources_seconds",
            self.freshness.resources_seconds,
        )?;
        validate_positive("freshness.power_seconds", self.freshness.power_seconds)?;
        validate_positive("freshness.network_seconds", self.freshness.network_seconds)?;
        validate_positive(
            "freshness.bluetooth_seconds",
            self.freshness.bluetooth_seconds,
        )?;
        validate_positive("freshness.media_seconds", self.freshness.media_seconds)?;
        validate_positive(
            "freshness.calendar_seconds",
            self.freshness.calendar_seconds,
        )?;
        validate_positive(
            "freshness.activity_seconds",
            self.freshness.activity_seconds,
        )?;
        validate_positive("freshness.timers_seconds", self.freshness.timers_seconds)?;

        validate_positive(
            "retry_backoff.compositor_seconds",
            self.retry_backoff.compositor_seconds,
        )?;
        validate_positive(
            "retry_backoff.power_seconds",
            self.retry_backoff.power_seconds,
        )?;
        validate_positive(
            "retry_backoff.network_seconds",
            self.retry_backoff.network_seconds,
        )?;
        validate_positive(
            "retry_backoff.bluetooth_seconds",
            self.retry_backoff.bluetooth_seconds,
        )?;
        validate_positive(
            "retry_backoff.media_seconds",
            self.retry_backoff.media_seconds,
        )?;
        validate_positive(
            "retry_backoff.calendar_seconds",
            self.retry_backoff.calendar_seconds,
        )?;
        validate_positive(
            "retry_backoff.activity_seconds",
            self.retry_backoff.activity_seconds,
        )?;

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfigReload {
    pub config: AppConfig,
    pub status: ReloadStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReloadStatus {
    Applied,
    RestartRequired { reasons: Vec<String> },
}

pub fn reload_runtime_config(current: &AppConfig, next: AppConfig) -> RuntimeConfigReload {
    let mut reasons = Vec::new();

    if current.freshness != next.freshness {
        reasons.push("freshness changes require restart".to_string());
    }
    if current.retry_backoff != next.retry_backoff {
        reasons.push("retry_backoff changes require restart".to_string());
    }
    if current.command_activity != next.command_activity {
        reasons.push("command_activity changes require restart".to_string());
    }

    if reasons.is_empty() {
        RuntimeConfigReload {
            config: next,
            status: ReloadStatus::Applied,
        }
    } else {
        RuntimeConfigReload {
            config: current.clone(),
            status: ReloadStatus::RestartRequired { reasons },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThresholdConfig {
    pub calendar_soon_minutes: u64,
    pub timer_soon_minutes: u64,
    pub battery_low_percent: u8,
    pub battery_critical_percent: u8,
    pub work_completed_seconds: u64,
    pub critical_snooze_seconds: u64,
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            calendar_soon_minutes: 15,
            timer_soon_minutes: 5,
            battery_low_percent: 15,
            battery_critical_percent: 7,
            work_completed_seconds: 30,
            critical_snooze_seconds: 300,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModuleConfig {
    pub full: Vec<ModuleName>,
    pub reduced: Vec<ModuleName>,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            full: vec![
                ModuleName::Workspaces,
                ModuleName::FocusedApp,
                ModuleName::FocusedTitle,
                ModuleName::Context,
                ModuleName::KeyboardLayout,
                ModuleName::Resources,
                ModuleName::Network,
                ModuleName::BluetoothAudio,
                ModuleName::Power,
                ModuleName::Clock,
            ],
            reduced: vec![
                ModuleName::Workspaces,
                ModuleName::FocusedTitle,
                ModuleName::CriticalWarning,
                ModuleName::Clock,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleName {
    Workspaces,
    FocusedApp,
    FocusedTitle,
    Context,
    KeyboardLayout,
    Resources,
    Network,
    BluetoothAudio,
    Power,
    Clock,
    CriticalWarning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FreshnessConfig {
    pub compositor_seconds: u64,
    pub resources_seconds: u64,
    pub power_seconds: u64,
    pub network_seconds: u64,
    pub bluetooth_seconds: u64,
    pub media_seconds: u64,
    pub calendar_seconds: u64,
    pub activity_seconds: u64,
    pub timers_seconds: u64,
}

impl Default for FreshnessConfig {
    fn default() -> Self {
        Self {
            compositor_seconds: 10,
            resources_seconds: 15,
            power_seconds: 30,
            network_seconds: 30,
            bluetooth_seconds: 30,
            media_seconds: 15,
            calendar_seconds: 60,
            activity_seconds: 45,
            timers_seconds: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RetryBackoffConfig {
    pub compositor_seconds: u64,
    pub power_seconds: u64,
    pub network_seconds: u64,
    pub bluetooth_seconds: u64,
    pub media_seconds: u64,
    pub calendar_seconds: u64,
    pub activity_seconds: u64,
}

impl Default for RetryBackoffConfig {
    fn default() -> Self {
        Self {
            compositor_seconds: 2,
            power_seconds: 8,
            network_seconds: 8,
            bluetooth_seconds: 8,
            media_seconds: 8,
            calendar_seconds: 30,
            activity_seconds: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommandActivityConfig {
    pub allowlist: Vec<CommandRule>,
}

impl Default for CommandActivityConfig {
    fn default() -> Self {
        Self {
            allowlist: vec![
                CommandRule::new("Cargo build", ["cargo build"]),
                CommandRule::new("Cargo test", ["cargo test"]),
                CommandRule::new("Cargo run", ["cargo run"]),
                CommandRule::new("npm test", ["npm test"]),
                CommandRule::new("pnpm test", ["pnpm test"]),
                CommandRule::new("Pytest", ["pytest"]),
                CommandRule::new("Make", ["make"]),
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRule {
    pub label: String,
    pub prefixes: Vec<String>,
}

impl CommandRule {
    fn new(label: &str, prefixes: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            label: label.to_string(),
            prefixes: prefixes.into_iter().map(str::to_string).collect(),
        }
    }
}

fn validate_percent(path: &str, value: u8) -> Result<()> {
    if value > 100 {
        return Err(ConfigError::Validation(format!(
            "{path} must be between 0 and 100, got {value}"
        ))
        .into());
    }

    Ok(())
}

fn validate_positive(path: &str, value: u64) -> Result<()> {
    if value == 0 {
        return Err(ConfigError::Validation(format!("{path} must be greater than zero")).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, ModuleName, ReloadStatus, reload_runtime_config};

    #[test]
    fn defaults_match_the_approved_urgency_policy() {
        let config = AppConfig::default();
        assert_eq!(config.primary_output.as_deref(), Some("DP-5"));
        assert_eq!(config.thresholds.calendar_soon_minutes, 15);
        assert_eq!(config.thresholds.timer_soon_minutes, 5);
        assert_eq!(config.thresholds.battery_low_percent, 15);
        assert_eq!(config.thresholds.battery_critical_percent, 7);
        assert_eq!(config.thresholds.critical_snooze_seconds, 300);
    }

    #[test]
    fn tracked_shape_parses_and_merges_defaults() {
        let config = AppConfig::from_toml(include_str!("../../../bar/config.toml")).unwrap();
        assert_eq!(config.primary_output.as_deref(), Some("DP-5"));
        assert_eq!(config.thresholds.work_completed_seconds, 30);
        assert_eq!(config.thresholds.critical_snooze_seconds, 300);
        assert_eq!(config.modules.reduced[0], ModuleName::Workspaces);
        assert_eq!(config.command_activity.allowlist.len(), 7);
    }

    #[test]
    fn critical_battery_must_be_below_low_battery() {
        let text = "[thresholds]\nbattery_low_percent=7\nbattery_critical_percent=15\n";
        let error = AppConfig::from_toml(text).unwrap_err().to_string();
        assert!(error.contains("battery_critical_percent must be lower"));
    }

    #[test]
    fn reload_runtime_config_applies_threshold_module_and_primary_output_changes() {
        let current = AppConfig::default();
        let mut next = current.clone();
        next.primary_output = Some("DP-4".to_string());
        next.thresholds.battery_low_percent = 12;
        next.modules.reduced = vec![ModuleName::Workspaces, ModuleName::Clock];

        let reloaded = reload_runtime_config(&current, next.clone());

        assert_eq!(reloaded.config, next);
        assert_eq!(reloaded.status, ReloadStatus::Applied);
    }

    #[test]
    fn reload_runtime_config_rejects_process_level_changes_and_preserves_current_config() {
        let current = AppConfig::default();
        let mut next = current.clone();
        next.thresholds.battery_low_percent = 12;
        next.freshness.network_seconds = 5;

        let reloaded = reload_runtime_config(&current, next);

        assert_eq!(reloaded.config, current);
        match reloaded.status {
            ReloadStatus::RestartRequired { reasons } => {
                assert!(
                    reasons
                        .iter()
                        .any(|reason: &String| reason.contains("freshness"))
                );
            }
            ReloadStatus::Applied => panic!("expected restart-required reload"),
        }
    }
}
