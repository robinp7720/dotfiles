use std::collections::BTreeMap;
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
use serde::Deserialize;

use crate::{AudioOutputState, AudioState, SourceHealth, SourceId, StateUpdate, SystemUpdate};

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
        outputs: Vec::new(),
    })
}

#[derive(Debug, Deserialize)]
struct PactlInfo {
    default_sink_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PactlSink {
    name: String,
    description: String,
    active_port: Option<String>,
    #[serde(default)]
    ports: Vec<PactlPort>,
    #[serde(default)]
    properties: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PactlPort {
    name: String,
    description: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}

fn parse_pactl_outputs(info: &str, sinks: &str) -> Result<Vec<AudioOutputState>> {
    let info: PactlInfo = serde_json::from_str(info).context("pactl info was not valid JSON")?;
    let sinks: Vec<PactlSink> =
        serde_json::from_str(sinks).context("pactl sink list was not valid JSON")?;

    Ok(sinks
        .into_iter()
        .map(|sink| {
            let active_port = sink
                .active_port
                .as_deref()
                .and_then(|active| sink.ports.iter().find(|port| port.name == active));
            AudioOutputState {
                is_default: info.default_sink_name.as_deref() == Some(sink.name.as_str()),
                alias: sink
                    .properties
                    .get("bluez5.alias")
                    .or_else(|| sink.properties.get("device.alias"))
                    .cloned(),
                port_description: active_port.map(|port| port.description.clone()),
                port_type: active_port.and_then(|port| port.kind.clone()),
                bus: sink.properties.get("device.bus").cloned(),
                name: sink.name,
                description: sink.description,
            }
        })
        .collect())
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
    let mut state = parse_wpctl_volume(&stdout)?;
    let info = command_stdout("pactl", ["-f", "json", "info"])?;
    let sinks = command_stdout("pactl", ["-f", "json", "list", "sinks"])?;
    state.outputs = parse_pactl_outputs(&info, &sinks)?;
    Ok(state)
}

fn command_stdout<const N: usize>(program: &str, args: [&str; N]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {program}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("{program} exited with {}: {stderr}", output.status);
    }
    String::from_utf8(output.stdout).with_context(|| format!("{program} output was not UTF-8"))
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
    use super::{parse_pactl_outputs, parse_wpctl_volume};
    use crate::{AudioOutputState, AudioState};

    #[test]
    fn parses_non_muted_wpctl_output() {
        assert_eq!(
            parse_wpctl_volume("Volume: 0.49").unwrap(),
            AudioState {
                volume_percent: Some(49),
                muted: false,
                outputs: Vec::new(),
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
                outputs: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_available_outputs_and_marks_the_default() {
        let info = r#"{"default_sink_name":"bluez_output.headphones"}"#;
        let sinks = r#"[
          {
            "name":"alsa_output.desktop",
            "description":"Built-in Audio Analog Stereo",
            "active_port":"analog-output-lineout",
            "ports":[{"name":"analog-output-lineout","description":"Line Out","type":"Line"}],
            "properties":{"device.bus":"pci"}
          },
          {
            "name":"bluez_output.headphones",
            "description":"Robin's Headphones",
            "active_port":null,
            "ports":[],
            "properties":{"device.bus":"bluetooth","bluez5.alias":"Headphones"}
          }
        ]"#;

        assert_eq!(
            parse_pactl_outputs(info, sinks).unwrap(),
            vec![
                AudioOutputState {
                    name: "alsa_output.desktop".to_string(),
                    description: "Built-in Audio Analog Stereo".to_string(),
                    alias: None,
                    port_description: Some("Line Out".to_string()),
                    port_type: Some("Line".to_string()),
                    bus: Some("pci".to_string()),
                    is_default: false,
                },
                AudioOutputState {
                    name: "bluez_output.headphones".to_string(),
                    description: "Robin's Headphones".to_string(),
                    alias: Some("Headphones".to_string()),
                    port_description: None,
                    port_type: None,
                    bus: Some("bluetooth".to_string()),
                    is_default: true,
                },
            ]
        );
    }

    #[test]
    fn accepts_an_empty_sink_list_without_a_default() {
        assert_eq!(
            parse_pactl_outputs(r#"{"default_sink_name":null}"#, "[]").unwrap(),
            Vec::new()
        );
    }
}
