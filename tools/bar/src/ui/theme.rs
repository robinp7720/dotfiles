use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use gtk::gdk;
use gtk4 as gtk;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePaths {
    pub style: PathBuf,
    pub colors: PathBuf,
}

pub fn resolve_theme_root(xdg_config_home: Option<&Path>, home: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = xdg_config_home {
        return Ok(root.join("cockpit-bar"));
    }

    if let Some(home) = home {
        return Ok(home.join(".config").join("cockpit-bar"));
    }

    Err(anyhow!("failed to resolve cockpit-bar theme directory"))
}

pub fn theme_paths_for_config(config_path: &Path) -> ThemePaths {
    let default_root = resolve_theme_root(
        std::env::var_os("XDG_CONFIG_HOME")
            .as_deref()
            .map(Path::new),
        dirs::home_dir().as_deref(),
    )
    .ok();
    let local_root = config_path.parent();

    ThemePaths {
        style: select_theme_file(local_root, default_root.as_deref(), "style.css"),
        colors: select_theme_file(local_root, default_root.as_deref(), "colors.css"),
    }
}

pub fn compose_css(paths: &ThemePaths) -> Result<String> {
    let style = std::fs::read_to_string(&paths.style)
        .with_context(|| format!("failed to read {}", paths.style.display()))?;
    let colors = std::fs::read_to_string(&paths.colors)
        .with_context(|| format!("failed to read {}", paths.colors.display()))?;

    Ok(format!("{style}\n{colors}"))
}

pub fn load_css(display: &gdk::Display, config_path: &Path) -> Result<gtk::CssProvider> {
    let provider = gtk::CssProvider::new();
    reload_css(&provider, config_path)?;
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Ok(provider)
}

pub fn reload_css(provider: &gtk::CssProvider, config_path: &Path) -> Result<()> {
    let css = compose_css(&theme_paths_for_config(config_path))?;
    provider.load_from_string(&css);
    Ok(())
}

fn select_theme_file(
    local_root: Option<&Path>,
    default_root: Option<&Path>,
    file_name: &str,
) -> PathBuf {
    local_root
        .map(|root| root.join(file_name))
        .filter(|candidate| candidate.exists())
        .or_else(|| default_root.map(|root| root.join(file_name)))
        .or_else(|| local_root.map(|root| root.join(file_name)))
        .unwrap_or_else(|| PathBuf::from(file_name))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{compose_css, resolve_theme_root, theme_paths_for_config};

    #[test]
    fn theme_paths_prefer_xdg_config_home() {
        let root = resolve_theme_root(Some(Path::new("/tmp/xdg")), Some(Path::new("/tmp/home")))
            .expect("xdg theme root");

        assert_eq!(root, PathBuf::from("/tmp/xdg").join("cockpit-bar"));
    }

    #[test]
    fn theme_paths_fall_back_to_home_config_directory() {
        let root = resolve_theme_root(None, Some(Path::new("/tmp/home"))).expect("home theme root");

        assert_eq!(root, PathBuf::from("/tmp/home/.config/cockpit-bar"));
    }

    #[test]
    fn theme_css_concatenates_stable_style_before_generated_colors() {
        let root = temp_dir("theme-css-order");
        fs::write(root.join("style.css"), "window { min-height: 44px; }\n").expect("style css");
        fs::write(root.join("colors.css"), "@define-color primary #123456;\n").expect("colors css");

        let css =
            compose_css(&theme_paths_for_config(&root.join("config.toml"))).expect("composed css");

        assert!(css.starts_with("window { min-height: 44px; }"));
        assert!(css.contains("@define-color primary #123456;"));
        assert!(
            css.find("window { min-height: 44px; }")
                .zip(css.find("@define-color primary #123456;"))
                .is_some_and(|(style, colors)| style < colors)
        );
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
}
