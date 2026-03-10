use crate::monitor::Monitor;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub profiles: HashMap<String, Profile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub monitors: Vec<MonitorConfig>,
    pub commands: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MonitorConfig {
    pub stable_id: String,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub transform: u8,
    pub primary: bool,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = get_config_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content).context("Failed to parse config.toml")?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = get_config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn get_profile_for_monitors(&self, monitors: &[Monitor]) -> Option<&Profile> {
        let hash = generate_hardware_hash(monitors);
        self.profiles.get(&hash)
    }

    pub fn add_profile(&mut self, name: String, monitors: &[Monitor]) {
        let hash = generate_hardware_hash(monitors);

        let monitor_configs = monitors
            .iter()
            .map(|m| MonitorConfig {
                stable_id: m.get_stable_id(),
                x: m.x,
                y: m.y,
                scale: m.scale.unwrap_or(1.0),
                transform: m.transform.unwrap_or(0),
                primary: false, // TODO: Detect primary
                width: m.width,
                height: m.height,
                refresh_rate: m.refresh_rate,
            })
            .collect();

        let profile = Profile {
            name,
            monitors: monitor_configs,
            commands: None, // User can manually add these later
        };

        self.profiles.insert(hash, profile);
    }

    fn validate(&self) -> Result<()> {
        for (hardware_hash, profile) in &self.profiles {
            let mut stable_ids = HashSet::new();

            for monitor in &profile.monitors {
                ensure!(
                    !monitor.stable_id.trim().is_empty(),
                    "Profile '{}' ({hardware_hash}) has a monitor with an empty stable_id",
                    profile.name
                );
                ensure!(
                    monitor.width > 0 && monitor.height > 0,
                    "Profile '{}' ({hardware_hash}) has invalid monitor dimensions for '{}'",
                    profile.name,
                    monitor.stable_id
                );
                ensure!(
                    monitor.scale.is_finite() && monitor.scale > 0.0,
                    "Profile '{}' ({hardware_hash}) has invalid scale for '{}'",
                    profile.name,
                    monitor.stable_id
                );
                ensure!(
                    monitor.transform <= 7,
                    "Profile '{}' ({hardware_hash}) has invalid transform for '{}'",
                    profile.name,
                    monitor.stable_id
                );
                ensure!(
                    stable_ids.insert(monitor.stable_id.clone()),
                    "Profile '{}' ({hardware_hash}) contains duplicate monitor stable_id '{}'",
                    profile.name,
                    monitor.stable_id
                );
            }

            if let Some(commands) = &profile.commands {
                for command in commands {
                    ensure!(
                        !command.trim().is_empty(),
                        "Profile '{}' ({hardware_hash}) contains an empty command",
                        profile.name
                    );
                }
            }
        }

        Ok(())
    }
}

pub fn generate_hardware_hash(monitors: &[Monitor]) -> String {
    // Sort stable IDs to ensure consistent hash regardless of plug order
    let mut ids: Vec<String> = monitors.iter().map(|m| m.get_stable_id()).collect();
    ids.sort();

    let joined = ids.join("|");
    let mut hasher = Sha256::new();
    hasher.update(joined);
    let result = hasher.finalize();
    format!("{:x}", result)
}

fn get_config_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("Could not find config dir")?;
    path.push("session-manager");
    path.push("config.toml");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_monitor(stable_id: &str) -> MonitorConfig {
        MonitorConfig {
            stable_id: stable_id.to_string(),
            x: 0,
            y: 0,
            scale: 1.0,
            transform: 0,
            primary: false,
            width: 1920,
            height: 1080,
            refresh_rate: 60000,
        }
    }

    #[test]
    fn validate_rejects_duplicate_stable_ids() {
        let config = Config {
            profiles: HashMap::from([(
                "hash".to_string(),
                Profile {
                    name: "desk".to_string(),
                    monitors: vec![sample_monitor("DP_1"), sample_monitor("DP_1")],
                    commands: None,
                },
            )]),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_invalid_transform() {
        let mut monitor = sample_monitor("DP_1");
        monitor.transform = 8;

        let config = Config {
            profiles: HashMap::from([(
                "hash".to_string(),
                Profile {
                    name: "desk".to_string(),
                    monitors: vec![monitor],
                    commands: None,
                },
            )]),
        };

        assert!(config.validate().is_err());
    }
}
