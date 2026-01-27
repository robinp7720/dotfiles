use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use sha2::{Sha256, Digest};
use crate::monitor::Monitor;

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
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
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
        
        let monitor_configs = monitors.iter().map(|m| MonitorConfig {
            stable_id: m.get_stable_id(),
            x: m.x,
            y: m.y,
            scale: m.scale.unwrap_or(1.0),
            transform: m.transform.unwrap_or(0),
            primary: false, // TODO: Detect primary
            width: m.width,
            height: m.height,
            refresh_rate: m.refresh_rate,
        }).collect();

        let profile = Profile {
            name,
            monitors: monitor_configs,
            commands: None, // User can manually add these later
        };

        self.profiles.insert(hash, profile);
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
