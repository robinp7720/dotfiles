//! This desktop's implementation of Luma's executable wallpaper protocol.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let result = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("Cannot run {program}"))?;
    if !result.status.success() {
        bail!(
            "{program}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(result.stdout)
}
fn expand(path: &str) -> PathBuf {
    path.strip_prefix("~/")
        .map(|rest| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest))
        .unwrap_or_else(|| path.into())
}
fn config_path() -> PathBuf {
    std::env::var_os("AWWW_WALLPAPER_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| expand("~/.dotfiles/scripts/awww_wallpapers.conf"))
}
fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.split('#').next()?.trim().split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim())
        .last()
}
fn update(text: &str, key: &str, path: &str) -> String {
    let mut found = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            if !found {
                lines.push(format!("{key}={path}"));
                found = true;
            }
        } else {
            lines.push(line.into());
        }
    }
    if !found {
        lines.push(format!("{key}={path}"));
    }
    lines.join("\n") + "\n"
}
fn save(path: &Path, text: &str) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(path.parent().context("No config directory")?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn outputs(text: &str) -> Result<Vec<Value>> {
    let monitors: Vec<(String, String, bool)> = if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
        .is_some()
    {
        let monitors: Value = serde_json::from_slice(&run("hyprctl", &["monitors", "-j"])?)?;
        monitors
            .as_array()
            .context("Invalid Hyprland monitors")?
            .iter()
            .filter_map(|m| {
                let id = m["name"].as_str()?;
                Some((
                    id.into(),
                    m["description"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(|label| format!("{label} ({id})"))
                        .unwrap_or_else(|| id.into()),
                    m["focused"].as_bool().unwrap_or(false),
                ))
            })
            .collect()
    } else if std::env::var_os("NIRI_SOCKET").is_some() {
        let focused: Value =
            serde_json::from_slice(&run("niri", &["msg", "--json", "focused-output"])?)?;
        let monitors: Value = serde_json::from_slice(&run("niri", &["msg", "--json", "outputs"])?)?;
        monitors
            .as_object()
            .context("Invalid Niri outputs")?
            .iter()
            .filter(|(_, m)| !m["current_mode"].is_null())
            .map(|(id, _)| (id.clone(), id.clone(), focused["name"].as_str() == Some(id)))
            .collect()
    } else {
        bail!("This adapter requires a Hyprland or Niri session");
    };
    Ok(monitors.into_iter().map(|(id, label, focused)| {
        let wallpaper = value(text, &id).or_else(|| value(text, "default")).map(expand);
        json!({"id": id, "label": label, "focused": focused, "wallpaper": wallpaper, "wallpaper_state": "configured"})
    }).collect())
}
fn image(request: &Value) -> Result<PathBuf> {
    let raw = request["path"].as_str().context("Missing image path")?;
    if !Path::new(raw).is_absolute() || raw.contains(['\n', '\r', '#']) {
        bail!(
            "Expected an absolute path without newline or # (unsupported by this desktop's config format)"
        );
    }
    let path = PathBuf::from(raw);
    if !path.is_file() {
        bail!("Image does not exist: {}", path.display());
    }
    Ok(path)
}
fn execute(request: &Value, theme: bool) -> Result<Value> {
    if request["version"] != 1 {
        bail!("Unsupported protocol version");
    }
    let operation = request["operation"].as_str().context("Missing operation")?;
    let path = config_path();
    // Lock a stable sibling, since saving replaces the config inode.
    let lock = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.with_extension("conf.lock"))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let text = fs::read_to_string(&path).context("Cannot read wallpaper configuration")?;
    if !theme && operation == "status" {
        return Ok(json!({"version": 1, "outputs": outputs(&text)?}));
    }
    if theme {
        let image = match operation {
            "apply" => image(request)?,
            "regenerate" => expand(
                value(&text, "theme")
                    .or_else(|| value(&text, "default"))
                    .context("No theme image configured")?,
            ),
            _ => bail!("Unsupported theme operation"),
        };
        let script = path
            .parent()
            .context("No script directory")?
            .join("awww_wallpaper_watcher.sh");
        // Script location is independent of an overridden configuration file.
        let script = if script.is_file() {
            script
        } else {
            expand("~/.dotfiles/scripts/awww_wallpaper_watcher.sh")
        };
        run(
            script.to_str().context("Invalid script path")?,
            &[
                "--theme-only",
                "--theme",
                image.to_str().context("Invalid image path")?,
            ],
        )?;
        if operation == "apply" {
            save(&path, &update(&text, "theme", image.to_str().unwrap()))?;
        }
    } else if operation == "apply" {
        let image = image(request)?;
        let output = request["output"].as_str().context("Missing output")?;
        if !outputs(&text)?.iter().any(|o| o["id"] == output) {
            bail!("Selected display is no longer connected");
        }
        run(
            "awww",
            &[
                "img",
                "--outputs",
                output,
                "--resize",
                "crop",
                "--transition-type",
                "simple",
                "--transition-step",
                "255",
                image.to_str().unwrap(),
            ],
        )?;
        let portable = std::env::var_os("HOME")
            .and_then(|home| {
                image
                    .strip_prefix(home)
                    .ok()
                    .map(|rest| format!("~/{}", rest.display()))
            })
            .unwrap_or_else(|| image.display().to_string());
        save(&path, &update(&text, output, &portable))?;
        run(
            "systemctl",
            &["--user", "restart", "awww-wallpaper.service"],
        )?;
    } else {
        bail!("Unsupported wallpaper operation");
    }
    Ok(json!({"version": 1, "ok": true}))
}
fn main() {
    let result = (|| -> Result<Value> {
        let mut input = String::new();
        std::io::stdin().take(65537).read_to_string(&mut input)?;
        if input.len() > 65536 {
            bail!("Request exceeds 64 KiB");
        }
        let request: Value = serde_json::from_str(&input)?;
        execute(
            &request,
            std::env::args().nth(1).as_deref() == Some("theme"),
        )
    })();
    match result {
        Ok(response) => println!("{response}"),
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacing_one_output_preserves_theme_default_and_comments() {
        let text = "# wallpapers\ndefault=~/a.png\ntheme=~/b.png\nDP-1=~/a.png\nDP-2=~/c.png\nDP-1=~/old.png\n";
        assert_eq!(
            update(text, "DP-1", "~/new.png"),
            "# wallpapers\ndefault=~/a.png\ntheme=~/b.png\nDP-1=~/new.png\nDP-2=~/c.png\n"
        );
    }
    #[test]
    fn rejects_ambiguous_config_paths_before_applying() {
        assert!(image(&json!({"path": "/tmp/a#b.png"})).is_err());
        assert!(image(&json!({"path": "relative.png"})).is_err());
    }
}
