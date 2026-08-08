use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub output: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,
    pub modes: DisplayModes,
    pub audio: AudioConfig,
    pub apps: AppCommands,
    #[serde(default)]
    pub power: PowerProfiles,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DisplayModes {
    pub home: String,
    pub gaming: String,
    pub media: String,
    #[serde(default = "default_scale")]
    pub scale: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AudioConfig {
    pub card: String,
    pub tv_profile: String,
    pub tv_sink: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppCommands {
    pub steam: CommandSpec,
    pub jellyfin: CommandSpec,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PowerProfiles {
    #[serde(default = "default_balanced")]
    pub home: String,
    #[serde(default = "default_performance")]
    pub gaming: String,
    #[serde(default = "default_balanced")]
    pub media: String,
}

impl Default for PowerProfiles {
    fn default() -> Self {
        Self {
            home: default_balanced(),
            gaming: default_performance(),
            media: default_balanced(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read TV mode config {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse TV mode config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(!self.output.trim().is_empty(), "output must not be empty");
        ensure!(
            !self.workspace.trim().is_empty(),
            "workspace must not be empty"
        );
        ensure!(
            self.modes.scale.is_finite() && self.modes.scale > 0.0,
            "display scale must be positive"
        );
        for (label, mode) in [
            ("home", &self.modes.home),
            ("gaming", &self.modes.gaming),
            ("media", &self.modes.media),
        ] {
            ensure!(
                valid_mode(mode),
                "{label} display mode must look like WIDTHxHEIGHT@REFRESH"
            );
        }
        for (label, value) in [
            ("audio card", &self.audio.card),
            ("TV audio profile", &self.audio.tv_profile),
            ("TV audio sink", &self.audio.tv_sink),
            ("Steam program", &self.apps.steam.program),
            ("Jellyfin program", &self.apps.jellyfin.program),
            ("home power profile", &self.power.home),
            ("gaming power profile", &self.power.gaming),
            ("media power profile", &self.power.media),
        ] {
            ensure!(!value.trim().is_empty(), "{label} must not be empty");
        }
        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tv-mode")
        .join("config.toml")
}

fn valid_mode(mode: &str) -> bool {
    let Some((size, refresh)) = mode.split_once('@') else {
        return false;
    };
    let Some((width, height)) = size.split_once('x') else {
        return false;
    };
    width.parse::<u32>().is_ok()
        && height.parse::<u32>().is_ok()
        && refresh.trim_end_matches("Hz").parse::<f64>().is_ok()
}

fn default_workspace() -> String {
    "tv".to_string()
}

fn default_scale() -> f64 {
    1.0
}

fn default_balanced() -> String {
    "balanced".to_string()
}

fn default_performance() -> String {
    "performance".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Config {
        Config {
            output: "HDMI-A-2".to_string(),
            workspace: "tv".to_string(),
            modes: DisplayModes {
                home: "3840x2160@60.0".to_string(),
                gaming: "1920x1080@119.88".to_string(),
                media: "3840x2160@60.0".to_string(),
                scale: 1.0,
            },
            audio: AudioConfig {
                card: "card".to_string(),
                tv_profile: "profile".to_string(),
                tv_sink: "sink".to_string(),
            },
            apps: AppCommands {
                steam: CommandSpec {
                    program: "steam".to_string(),
                    args: vec!["steam://open/bigpicture".to_string()],
                },
                jellyfin: CommandSpec {
                    program: "jellyfin-desktop".to_string(),
                    args: vec!["--fullscreen".to_string(), "--tv".to_string()],
                },
            },
            power: PowerProfiles::default(),
        }
    }

    #[test]
    fn validates_expected_config() {
        fixture().validate().unwrap();
    }

    #[test]
    fn rejects_invalid_display_mode() {
        let mut config = fixture();
        config.modes.gaming = "fast".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_shell_only_app_command() {
        let mut config = fixture();
        config.apps.steam.program.clear();
        assert!(config.validate().is_err());
    }
}
