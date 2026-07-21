use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    CalendarAgenda, CalendarAgendaEvent, CalendarEvent, SourceHealth, SourceId, StateUpdate,
    SystemUpdate,
};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarMonthRequest {
    pub year: i32,
    pub month: u32,
}

impl CalendarMonthRequest {
    pub fn new(year: i32, month: u32) -> Result<Self> {
        if !(1..=12).contains(&month) {
            bail!("calendar month must be between 1 and 12");
        }
        Ok(Self { year, month })
    }

    fn range(self) -> (String, String) {
        let (next_year, next_month) = if self.month == 12 {
            (self.year + 1, 1)
        } else {
            (self.year, self.month + 1)
        };
        (
            format!("{:04}-{:02}-01", self.year, self.month),
            format!("{next_year:04}-{next_month:02}-01"),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarRecord {
    pub healthy: bool,
    pub id: String,
    pub title: String,
    pub location: Option<String>,
    pub start_epoch: i64,
    pub end_epoch: i64,
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

#[derive(Clone, Debug, Deserialize)]
struct CalendarAgendaPayload {
    healthy: bool,
    #[serde(default)]
    error: Option<String>,
    range_start: String,
    range_end: String,
    #[serde(default)]
    events: Vec<CalendarAgendaEvent>,
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
    if record.end_epoch <= 0 {
        bail!("end_epoch must be positive");
    }
    if record.end_epoch < record.start_epoch {
        bail!("end_epoch must be greater than or equal to start_epoch");
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

pub fn spawn_calendar_agenda_source(
    script_path: PathBuf,
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> (Sender<CalendarMonthRequest>, thread::JoinHandle<()>) {
    let (request_sender, request_receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut active_request = None;
        let mut elapsed = POLL_INTERVAL;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            match request_receiver.recv_timeout(CANCEL_POLL_INTERVAL) {
                Ok(request) => {
                    active_request = Some(latest_request(request, &request_receiver));
                    elapsed = POLL_INTERVAL;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    elapsed = elapsed.saturating_add(CANCEL_POLL_INTERVAL);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if elapsed < POLL_INTERVAL {
                continue;
            }
            let Some(request) = active_request else {
                continue;
            };
            if !publish_agenda(&script_path, request, &sender) {
                break;
            }
            elapsed = Duration::ZERO;
        }
    });
    (request_sender, handle)
}

fn latest_request(
    first: CalendarMonthRequest,
    receiver: &Receiver<CalendarMonthRequest>,
) -> CalendarMonthRequest {
    let mut latest = first;
    loop {
        match receiver.try_recv() {
            Ok(request) => latest = request,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return latest,
        }
    }
}

fn publish_agenda(
    script_path: &Path,
    request: CalendarMonthRequest,
    sender: &Sender<StateUpdate>,
) -> bool {
    match read_calendar_agenda(script_path, request) {
        Ok(agenda) => {
            sender
                .send(StateUpdate::System(SystemUpdate::CalendarAgenda(Some(
                    agenda,
                ))))
                .is_ok()
                && sender
                    .send(StateUpdate::Health {
                        source: SourceId::CalendarAgenda,
                        health: SourceHealth::Healthy,
                    })
                    .is_ok()
        }
        Err(error) => sender
            .send(StateUpdate::Health {
                source: SourceId::CalendarAgenda,
                health: SourceHealth::Disconnected {
                    message: error.to_string(),
                },
            })
            .is_ok(),
    }
}

fn read_calendar_agenda(
    script_path: &Path,
    request: CalendarMonthRequest,
) -> Result<CalendarAgenda> {
    let (range_start, range_end) = request.range();
    let output = Command::new(script_path)
        .args([
            "--agenda-json",
            "--from",
            range_start.as_str(),
            "--to",
            range_end.as_str(),
        ])
        .output()
        .with_context(|| format!("failed to execute {}", script_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "calendar agenda script failed{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }

    let text = String::from_utf8(output.stdout).context("calendar agenda output was not UTF-8")?;
    parse_calendar_agenda_json(text.trim(), request)
}

pub fn parse_calendar_agenda_json(
    text: &str,
    request: CalendarMonthRequest,
) -> Result<CalendarAgenda> {
    let (range_start, range_end) = request.range();
    let mut payload: CalendarAgendaPayload =
        serde_json::from_str(text).context("failed to parse calendar agenda JSON")?;
    if !payload.healthy {
        bail!(
            "{}",
            payload
                .error
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| "calendar agenda backend is unavailable".to_string())
        );
    }
    if payload.range_start != range_start || payload.range_end != range_end {
        bail!("calendar agenda response range does not match request");
    }
    validate_agenda_events(&payload.events)?;
    payload.events.sort_by(|left, right| {
        (left.start_epoch, &left.title, &left.id).cmp(&(right.start_epoch, &right.title, &right.id))
    });
    payload.events.dedup_by(|left, right| left.id == right.id);
    Ok(CalendarAgenda {
        year: request.year,
        month: request.month,
        events: payload.events,
    })
}

fn validate_agenda_events(events: &[CalendarAgendaEvent]) -> Result<()> {
    for event in events {
        if event.id.trim().is_empty() {
            bail!("calendar agenda event id must not be empty");
        }
        if event.title.trim().is_empty() {
            bail!("calendar agenda event title must not be empty");
        }
        if event.start_epoch <= 0 || event.end_epoch <= 0 {
            bail!("calendar agenda event timestamps must be positive");
        }
        if event.end_epoch < event.start_epoch {
            bail!("calendar agenda event end must not precede start");
        }
    }
    Ok(())
}

fn publish_once(script_path: &Path, sender: &Sender<StateUpdate>) -> bool {
    match read_calendar(script_path) {
        Ok(CalendarSourceOutcome::Event(record)) => {
            let event = CalendarEvent {
                id: record.id,
                title: record.title,
                location: record.location,
                start_epoch: record.start_epoch,
                end_epoch: Some(record.end_epoch),
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
