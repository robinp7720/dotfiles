use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::{BrightnessState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

const BRIGHTNESS_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub fn parse_brightnessctl_output(text: &str) -> Result<BrightnessState> {
    for line in text.lines() {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() < 5 || fields[1].trim() != "backlight" {
            continue;
        }

        let device = fields[0].trim();
        let raw_percent = fields[3].trim().trim_end_matches('%');
        let percent = raw_percent
            .parse::<u8>()
            .with_context(|| format!("failed to parse brightness percentage '{raw_percent}'"))?;
        return Ok(BrightnessState {
            device: Some(device.to_string()),
            percent: Some(percent.min(100)),
        });
    }

    Ok(BrightnessState::default())
}

fn read_brightness_state() -> Result<BrightnessState> {
    let output = Command::new("brightnessctl")
        .args(["--class=backlight", "-m"])
        .output()
        .context("failed to execute brightnessctl --class=backlight -m")?;
    let stdout = String::from_utf8(output.stdout).context("brightnessctl output was not UTF-8")?;
    if output.status.success() {
        return parse_brightnessctl_output(&stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No devices found") || stderr.contains("No such device") {
        return Ok(BrightnessState::default());
    }

    bail!("brightnessctl query failed: {}", stderr.trim())
}

fn publish_brightness_snapshot(
    sender: &Sender<StateUpdate>,
    cancelled: &Arc<AtomicBool>,
) -> Result<()> {
    let state = read_brightness_state()?;
    if sender
        .send(StateUpdate::System(SystemUpdate::Brightness(state)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub fn spawn_brightness_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    super::SourceSupervisor::spawn(cancelled.clone(), BRIGHTNESS_POLL_INTERVAL, move || {
        match publish_brightness_snapshot(&sender, &cancelled) {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Brightness,
                    health: SourceHealth::Disconnected {
                        message: error.to_string(),
                    },
                });
                Err(error)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::parse_brightnessctl_output;
    use crate::BrightnessState;

    #[test]
    fn parses_first_real_backlight_and_ignores_led_devices() {
        let output = concat!(
            "input2::capslock,leds,0,0%,1\n",
            "amdgpu_bl1,backlight,267,67%,400\n",
            "intel_backlight,backlight,45,45%,100\n",
        );

        assert_eq!(
            parse_brightnessctl_output(output).unwrap(),
            BrightnessState {
                device: Some("amdgpu_bl1".to_string()),
                percent: Some(67),
            }
        );
    }

    #[test]
    fn no_real_backlight_is_healthy_unavailable_state() {
        assert_eq!(
            parse_brightnessctl_output("input2::capslock,leds,0,0%,1\n").unwrap(),
            BrightnessState::default()
        );
    }
}
