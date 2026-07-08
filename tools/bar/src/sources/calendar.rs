use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{CalendarEvent, SourceHealth, SourceId, StateUpdate, SystemUpdate};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarRecord {
    pub healthy: bool,
    pub id: String,
    pub title: String,
    pub location: Option<String>,
    pub start_epoch: i64,
    pub end_epoch: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CalendarPayload {
    healthy: bool,
    #[serde(default)]
    empty: bool,
    #[serde(default)]
    error: Option<String>,
}

enum CalendarSourceOutcome {
    Event(CalendarRecord),
    Empty,
    Disconnected(String),
}

pub fn parse_calendar_json(text: &str) -> Result<CalendarRecord> {
    let record: CalendarRecord =
        serde_json::from_str(text).context("failed to parse calendar JSON")?;
    if record.start_epoch <= 0 {
        bail!("start_epoch must be positive");
    }
    if let Some(end_epoch) = record.end_epoch {
        if end_epoch <= 0 {
            bail!("end_epoch must be positive");
        }
        if end_epoch < record.start_epoch {
            bail!("end_epoch must be greater than or equal to start_epoch");
        }
    }

    Ok(record)
}

pub fn spawn_calendar_source(
    script_path: PathBuf,
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            if !publish_once(&script_path, &sender) {
                break;
            }

            if !sleep_until_next_poll(&cancelled) {
                break;
            }
        }
    })
}

fn publish_once(script_path: &Path, sender: &Sender<StateUpdate>) -> bool {
    match read_calendar(script_path) {
        Ok(CalendarSourceOutcome::Event(record)) => {
            let event = CalendarEvent {
                id: record.id,
                title: record.title,
                location: record.location,
                start_epoch: record.start_epoch,
                end_epoch: record.end_epoch,
                changed_at: 0,
            };
            sender
                .send(StateUpdate::System(SystemUpdate::Calendar(Some(event))))
                .is_ok()
                && sender
                    .send(StateUpdate::Health {
                        source: SourceId::Calendar,
                        health: SourceHealth::Healthy,
                    })
                    .is_ok()
        }
        Ok(CalendarSourceOutcome::Empty) => {
            sender
                .send(StateUpdate::System(SystemUpdate::Calendar(None)))
                .is_ok()
                && sender
                    .send(StateUpdate::Health {
                        source: SourceId::Calendar,
                        health: SourceHealth::Healthy,
                    })
                    .is_ok()
        }
        Ok(CalendarSourceOutcome::Disconnected(message)) => sender
            .send(StateUpdate::Health {
                source: SourceId::Calendar,
                health: SourceHealth::Disconnected { message },
            })
            .is_ok(),
        Err(error) => sender
            .send(StateUpdate::Health {
                source: SourceId::Calendar,
                health: SourceHealth::Disconnected {
                    message: error.to_string(),
                },
            })
            .is_ok(),
    }
}

fn read_calendar(script_path: &Path) -> Result<CalendarSourceOutcome, anyhow::Error> {
    let output = Command::new(script_path)
        .arg("--json")
        .output()
        .with_context(|| format!("failed to execute {}", script_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let status = output.status.code().map_or_else(
            || "terminated by signal".to_string(),
            |code| format!("exit code {code}"),
        );
        let message = if stderr.is_empty() {
            format!("calendar script failed with {status}")
        } else {
            format!("calendar script failed with {status}: {stderr}")
        };
        return Ok(CalendarSourceOutcome::Disconnected(message));
    }

    let stdout =
        String::from_utf8(output.stdout).context("calendar script output was not UTF-8")?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(CalendarSourceOutcome::Disconnected(
            "calendar script returned empty output".to_string(),
        ));
    }

    let payload: CalendarPayload =
        serde_json::from_str(trimmed).context("failed to parse calendar source payload")?;
    if !payload.healthy {
        return Ok(CalendarSourceOutcome::Disconnected(
            payload
                .error
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| "calendar source reported an unhealthy backend".to_string()),
        ));
    }
    if payload.empty {
        return Ok(CalendarSourceOutcome::Empty);
    }

    Ok(CalendarSourceOutcome::Event(parse_calendar_json(trimmed)?))
}

fn sleep_until_next_poll(cancelled: &AtomicBool) -> bool {
    let mut elapsed = Duration::ZERO;
    while elapsed < POLL_INTERVAL {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(CANCEL_POLL_INTERVAL);
        elapsed += CANCEL_POLL_INTERVAL;
    }
    true
}
