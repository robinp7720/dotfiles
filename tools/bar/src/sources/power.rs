use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    PowerProfile, PowerState, SourceHealth, SourceId, StateUpdate, SystemUpdate, ThresholdConfig,
};

use super::SourceSupervisor;

const POWER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";
const UPOWER_DESTINATION: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_INTERFACE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const POWER_PROFILES_DESTINATION: &str = "net.hadess.PowerProfiles";
const POWER_PROFILES_PATH: &str = "/net/hadess/PowerProfiles";
const POWER_PROFILES_INTERFACE: &str = "net.hadess.PowerProfiles";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatterySnapshot {
    battery_percent: Option<u8>,
    charging: bool,
}

pub fn battery_severity(
    battery_percent: Option<u8>,
    charging: bool,
    thresholds: &ThresholdConfig,
) -> Option<&'static str> {
    if charging {
        return None;
    }

    let percent = battery_percent?;
    if percent <= thresholds.battery_critical_percent {
        Some("critical")
    } else if percent <= thresholds.battery_low_percent {
        Some("warning")
    } else {
        None
    }
}

pub fn spawn_power_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    SourceSupervisor::spawn(cancelled.clone(), POWER_POLL_INTERVAL, move || {
        match publish_power_snapshot(&sender, &cancelled) {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Power,
                    health: SourceHealth::Disconnected {
                        message: error.to_string(),
                    },
                });
                Err(error)
            }
        }
    })
}

fn publish_power_snapshot(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<()> {
    let battery = read_power_battery()?;
    let profile = read_power_profile().unwrap_or_default();
    let update = StateUpdate::System(SystemUpdate::Power(PowerState {
        battery_percent: battery.battery_percent,
        charging: battery.charging,
        profile,
        changed_at: 0,
    }));

    if sender.send(update).is_err() {
        cancelled.store(true, Ordering::Relaxed);
    }

    Ok(())
}

fn read_power_battery() -> Result<BatterySnapshot> {
    match read_upower_battery() {
        Ok(snapshot) => Ok(snapshot),
        Err(_) => read_sysfs_battery(Path::new(POWER_SUPPLY_PATH)),
    }
}

fn read_upower_battery() -> Result<BatterySnapshot> {
    let connection =
        Connection::system().context("failed to connect to system D-Bus for UPower")?;
    let upower = Proxy::new(
        &connection,
        UPOWER_DESTINATION,
        UPOWER_PATH,
        UPOWER_INTERFACE,
    )
    .context("failed to build UPower proxy")?;
    let display_device: OwnedObjectPath = upower
        .call("GetDisplayDevice", &())
        .context("failed to query UPower display device")?;

    let device = Proxy::new(
        &connection,
        UPOWER_DESTINATION,
        display_device.as_str(),
        UPOWER_DEVICE_INTERFACE,
    )
    .context("failed to build UPower display device proxy")?;

    let is_present: bool = device
        .get_property("IsPresent")
        .context("failed to read UPower IsPresent")?;
    if !is_present {
        return Ok(BatterySnapshot {
            battery_percent: None,
            charging: false,
        });
    }

    let percentage: f64 = device
        .get_property("Percentage")
        .context("failed to read UPower Percentage")?;
    let state: u32 = device
        .get_property("State")
        .context("failed to read UPower State")?;

    Ok(BatterySnapshot {
        battery_percent: Some(clamp_percent_f64(percentage)),
        charging: matches!(state, 1 | 4 | 5),
    })
}

fn read_sysfs_battery(root: &Path) -> Result<BatterySnapshot> {
    let paths = battery_paths(root)?;
    if paths.is_empty() {
        return Ok(BatterySnapshot {
            battery_percent: None,
            charging: false,
        });
    }

    let battery_percent = weighted_capacity(&paths)?;
    let statuses = paths
        .iter()
        .map(|path| read_trimmed(path.join("status")))
        .collect::<Result<Vec<_>>>()?;
    let charging = statuses
        .iter()
        .any(|status| matches!(status.as_str(), "Charging" | "Full"));

    Ok(BatterySnapshot {
        battery_percent,
        charging,
    })
}

fn read_power_profile() -> Result<PowerProfile> {
    read_power_profile_dbus().or_else(|_| read_power_profile_command())
}

fn read_power_profile_dbus() -> Result<PowerProfile> {
    let connection =
        Connection::system().context("failed to connect to system D-Bus for power profiles")?;
    let proxy = Proxy::new(
        &connection,
        POWER_PROFILES_DESTINATION,
        POWER_PROFILES_PATH,
        POWER_PROFILES_INTERFACE,
    )
    .context("failed to build power profiles proxy")?;
    let profile: String = proxy
        .get_property("ActiveProfile")
        .context("failed to read power profile ActiveProfile")?;
    Ok(map_power_profile(&profile))
}

fn read_power_profile_command() -> Result<PowerProfile> {
    let output = Command::new("powerprofilesctl")
        .arg("get")
        .output()
        .context("failed to execute powerprofilesctl get")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("powerprofilesctl get failed: {stderr}");
    }

    let stdout =
        String::from_utf8(output.stdout).context("powerprofilesctl output was not UTF-8")?;
    let profile = stdout.trim();
    if profile.is_empty() {
        bail!("powerprofilesctl get returned an empty profile");
    }

    Ok(map_power_profile(profile))
}

fn map_power_profile(value: &str) -> PowerProfile {
    match value.trim() {
        "performance" => PowerProfile::Performance,
        "power-saver" => PowerProfile::PowerSaver,
        _ => PowerProfile::Balanced,
    }
}

fn battery_paths(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut batteries = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if read_trimmed(path.join("type"))?.eq_ignore_ascii_case("battery") {
            batteries.push(path);
        }
    }
    batteries.sort();
    Ok(batteries)
}

fn weighted_capacity(paths: &[PathBuf]) -> Result<Option<u8>> {
    let mut current_total: u64 = 0;
    let mut full_total: u64 = 0;

    for path in paths {
        let current = read_optional_u64(path.join("energy_now"))?
            .or_else(|| read_optional_u64(path.join("charge_now")).ok().flatten());
        let full = read_optional_u64(path.join("energy_full"))?
            .or_else(|| read_optional_u64(path.join("charge_full")).ok().flatten());

        if let (Some(current), Some(full)) = (current, full)
            && full > 0
        {
            current_total = current_total.saturating_add(current);
            full_total = full_total.saturating_add(full);
        }
    }

    if full_total > 0 {
        let percent = ((current_total * 100) + (full_total / 2)) / full_total;
        return Ok(Some(clamp_percent_u64(percent)));
    }

    let mut capacities = Vec::new();
    for path in paths {
        if let Some(capacity) = read_optional_u64(path.join("capacity"))? {
            capacities.push(capacity);
        }
    }

    if capacities.is_empty() {
        return Ok(None);
    }

    let sum: u64 = capacities.iter().copied().sum();
    let percent = (sum + (u64::try_from(capacities.len()).unwrap_or(0) / 2))
        / u64::try_from(capacities.len()).unwrap_or(1);
    Ok(Some(clamp_percent_u64(percent)))
}

fn read_trimmed(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text.trim().to_string())
}

fn read_optional_u64(path: impl AsRef<Path>) -> Result<Option<u64>> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                trimmed
                    .parse::<u64>()
                    .map(Some)
                    .with_context(|| format!("failed to parse integer from {}", path.display()))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(anyhow!(error)).with_context(|| format!("failed to read {}", path.display()))
        }
    }
}

fn clamp_percent_f64(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

fn clamp_percent_u64(value: u64) -> u8 {
    u8::try_from(value.min(100)).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use crate::{PowerProfile, ThresholdConfig};

    use super::{battery_severity, map_power_profile};

    #[test]
    fn charging_battery_suppresses_warning_severity() {
        let thresholds = ThresholdConfig::default();

        assert_eq!(battery_severity(Some(6), true, &thresholds), None);
        assert_eq!(battery_severity(Some(15), true, &thresholds), None);
    }

    #[test]
    fn battery_severity_uses_exact_threshold_boundaries() {
        let thresholds = ThresholdConfig::default();

        assert_eq!(battery_severity(Some(16), false, &thresholds), None);
        assert_eq!(
            battery_severity(Some(15), false, &thresholds),
            Some("warning")
        );
        assert_eq!(
            battery_severity(Some(7), false, &thresholds),
            Some("critical")
        );
        assert_eq!(
            battery_severity(Some(6), false, &thresholds),
            Some("critical")
        );
    }

    #[test]
    fn power_profile_mapping_covers_expected_profiles() {
        assert_eq!(map_power_profile("performance"), PowerProfile::Performance);
        assert_eq!(map_power_profile("balanced"), PowerProfile::Balanced);
        assert_eq!(map_power_profile("power-saver"), PowerProfile::PowerSaver);
    }
}
