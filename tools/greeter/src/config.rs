use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub preferred_output: String,
    pub panel_edge: String,
    pub wallpaper: PathBuf,
    pub theme: PathBuf,
    pub state: PathBuf,
    pub session_dirs: Vec<PathBuf>,
    pub initial_session: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            preferred_output: "DP-3".into(),
            panel_edge: "left".into(),
            wallpaper: "/var/cache/shell-greeter/current/wallpaper".into(),
            theme: "/var/cache/shell-greeter/current/theme.css".into(),
            state: "/var/lib/shell-greeter/state.json".into(),
            session_dirs: vec![
                "/usr/share/wayland-sessions".into(),
                "/usr/share/xsessions".into(),
            ],
            initial_session: "wayland:hyprland.desktop".into(),
        }
    }
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut config: Self = toml::from_str(
            &fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?,
        )?;
        ensure!(
            ["left", "right"].contains(&config.panel_edge.as_str()),
            "panel_edge must be left or right"
        );
        ensure!(
            !config.session_dirs.is_empty(),
            "session_dirs cannot be empty"
        );
        for path in [&config.wallpaper, &config.theme, &config.state]
            .into_iter()
            .chain(config.session_dirs.iter())
        {
            ensure!(
                path.is_absolute(),
                "Paths must be absolute: {}",
                path.display()
            );
        }
        // Resolve the published pair once: a concurrent publication cannot mix generations.
        if config.theme.parent() == config.wallpaper.parent()
            && let Some(parent) = config.theme.parent()
            && let Ok(generation) = fs::canonicalize(parent)
        {
            config.theme = generation.join(config.theme.file_name().unwrap());
            config.wallpaper = generation.join(config.wallpaper.file_name().unwrap());
        }
        Ok(config)
    }
}

#[derive(Default, Deserialize, Serialize)]
pub struct State {
    pub user: String,
    pub sessions: BTreeMap<String, String>,
}
impl State {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let tmp = path.with_extension("json.new");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(&serde_json::to_vec(self)?)?;
        file.sync_all()?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_state_is_safe_and_success_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        fs::write(&path, "broken").unwrap();
        assert!(State::load(&path).user.is_empty());
        let state = State {
            user: "robin".into(),
            sessions: BTreeMap::from([("robin".into(), "wayland:niri.desktop".into())]),
        };
        state.save(&path).unwrap();
        assert_eq!(State::load(&path).sessions["robin"], "wayland:niri.desktop");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn invalid_config_fails_before_display_or_socket_access() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(tmp.path(), "panel_edge = 'middle'").unwrap();
        assert!(Config::load(tmp.path()).is_err());
        fs::write(tmp.path(), "wallpaper = 'relative'").unwrap();
        assert!(Config::load(tmp.path()).is_err());
        fs::write(tmp.path(), "").unwrap();
        assert!(Config::load(tmp.path()).is_ok());
    }
}
