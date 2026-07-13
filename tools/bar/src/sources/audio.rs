use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, RecvTimeoutError, Sender},
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::{AudioState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

const AUDIO_EVENT_TIMEOUT: Duration = Duration::from_millis(250);
const AUDIO_RESTART_DELAY: Duration = Duration::from_secs(1);

fn parse_wpctl_volume(text: &str) -> Result<AudioState> {
    let trimmed = text.trim();
    let remainder = trimmed
        .strip_prefix("Volume:")
        .context("wpctl output did not start with 'Volume:'")?
        .trim();
    let muted = remainder.contains("[MUTED]");
    let number = remainder
        .split_whitespace()
        .find(|token| !token.starts_with('['))
        .context("wpctl output did not include a numeric volume")?;
    let raw = number
        .parse::<f64>()
        .with_context(|| format!("failed to parse wpctl volume '{number}'"))?;
    let percent = (raw * 100.0).round().clamp(0.0, 150.0) as u8;

    Ok(AudioState {
        volume_percent: Some(percent),
        muted,
    })
}

fn publish_audio_snapshot(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<()> {
    let state = read_audio_state()?;
    if sender
        .send(StateUpdate::System(SystemUpdate::Audio(state)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn read_audio_state() -> Result<AudioState> {
    let output = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .context("failed to execute wpctl get-volume @DEFAULT_AUDIO_SINK@")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("wpctl get-volume failed: {stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("wpctl output was not UTF-8")?;
    parse_wpctl_volume(&stdout)
}

fn spawn_audio_bridge() -> Result<(Child, mpsc::Receiver<Result<String>>)> {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute pactl subscribe")?;
    let stdout = child
        .stdout
        .take()
        .context("pactl subscribe did not provide stdout")?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || pump_lines(stdout, tx));
    Ok((child, rx))
}

fn pump_lines(stdout: ChildStdout, sender: mpsc::Sender<Result<String>>) {
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                if sender.send(Ok(line)).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error.into()));
                break;
            }
        }
    }
}

fn run_audio_worker(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<bool> {
    publish_audio_snapshot(sender, cancelled)?;

    let (mut child, receiver) = spawn_audio_bridge()?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            kill_child(&mut child);
            return Ok(false);
        }

        match receiver.recv_timeout(AUDIO_EVENT_TIMEOUT) {
            Ok(Ok(_line)) => publish_audio_snapshot(sender, cancelled)?,
            Ok(Err(error)) => {
                kill_child(&mut child);
                return Err(error);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = child.try_wait().context("failed to poll pactl subscribe")? {
                    if status.success() {
                        return Ok(true);
                    }
                    return Err(anyhow!("pactl subscribe exited with {status}"));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Ok(true);
            }
        }
    }
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn spawn_audio_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    super::SourceSupervisor::spawn(cancelled.clone(), AUDIO_RESTART_DELAY, move || {
        match run_audio_worker(&sender, &cancelled) {
            Ok(healthy) => Ok(healthy),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Audio,
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
    use super::parse_wpctl_volume;
    use crate::AudioState;

    #[test]
    fn parses_non_muted_wpctl_output() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.49").unwrap(),
            AudioState {
                volume_percent: Some(49),
                muted: false,
            }
        );
    }

    #[test]
    fn parses_muted_wpctl_output() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.31 [MUTED]").unwrap(),
            AudioState {
                volume_percent: Some(31),
                muted: true,
            }
        );
    }
}
