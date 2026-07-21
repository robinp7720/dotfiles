use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cockpit_bar::{
    CalendarAgenda, CalendarAgendaEvent, CalendarEvent, CalendarMonthRequest, FreshnessConfig,
    SourceHealth, SourceId, StateStore, StateUpdate, SystemUpdate, parse_calendar_agenda_json,
    parse_calendar_json, spawn_calendar_agenda_source, spawn_calendar_source,
};

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"))
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod");
    }
}

#[test]
fn parse_calendar_json_rejects_non_positive_start_epoch() {
    let error = parse_calendar_json(
        r#"{"healthy":true,"id":"review","title":"Review","location":"Room 2","start_epoch":0,"end_epoch":1800002400}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("start_epoch must be positive"));
}

#[test]
fn parse_calendar_json_rejects_end_before_start() {
    let error = parse_calendar_json(
        r#"{"healthy":true,"id":"review","title":"Review","location":"Room 2","start_epoch":1800002400,"end_epoch":1800000600}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("end_epoch must be greater than or equal to start_epoch"));
}

#[test]
fn parse_calendar_json_rejects_missing_end_epoch() {
    assert!(
        parse_calendar_json(
            r#"{"healthy":true,"id":"review","title":"Review","location":"Room 2","start_epoch":1800000600}"#,
        )
        .is_err()
    );
}

#[test]
fn parse_calendar_json_rejects_non_positive_end_epoch() {
    let error = parse_calendar_json(
        r#"{"healthy":true,"id":"review","title":"Review","location":"Room 2","start_epoch":1800000600,"end_epoch":0}"#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("end_epoch must be positive"));
}

#[test]
fn parse_calendar_json_preserves_json_escaped_control_characters() {
    let record = parse_calendar_json(
        r#"{"healthy":true,"id":"special","title":"Planning \"A\"\tB\rC\nD\\E","location":"Room \"2\"\tEast\rWing\nDesk\\7","start_epoch":1800000600,"end_epoch":1800002400}"#,
    )
    .expect("parse escaped calendar record");

    assert_eq!(record.title, "Planning \"A\"\tB\rC\nD\\E");
    assert_eq!(
        record.location.as_deref(),
        Some("Room \"2\"\tEast\rWing\nDesk\\7")
    );
    assert_eq!(record.end_epoch, 1_800_002_400);
}

#[test]
fn calendar_source_publishes_calendar_event_and_health() {
    let temp_dir = unique_temp_dir("calendar-source");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let script_path = temp_dir.join("next_event.sh");
    write_script(
        &script_path,
        r#"#!/usr/bin/env bash
printf '%s\n' '{"healthy":true,"id":"gcalcli:1800000600:Design review","title":"Design review","location":"Room 2","start_epoch":1800000600,"end_epoch":1800002400}'
"#,
    );

    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = spawn_calendar_source(script_path, sender, cancelled.clone());

    let first = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("calendar update");
    let second = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("health update");

    cancelled.store(true, Ordering::Relaxed);
    handle.join().expect("join calendar source");
    fs::remove_dir_all(&temp_dir).ok();

    assert_eq!(
        first,
        StateUpdate::System(SystemUpdate::Calendar(Some(CalendarEvent {
            id: "gcalcli:1800000600:Design review".to_string(),
            title: "Design review".to_string(),
            location: Some("Room 2".to_string()),
            start_epoch: 1_800_000_600,
            end_epoch: Some(1_800_002_400),
            changed_at: 0,
        })))
    );
    assert_eq!(
        second,
        StateUpdate::Health {
            source: SourceId::Calendar,
            health: SourceHealth::Healthy,
        }
    );
}

#[test]
fn disconnected_calendar_payload_does_not_clear_fresh_state() {
    let temp_dir = unique_temp_dir("calendar-disconnected");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let script_path = temp_dir.join("next_event.sh");
    write_script(
        &script_path,
        r#"#!/usr/bin/env bash
printf '%s\n' '{"healthy":false,"error":"gcalcli unavailable"}'
"#,
    );

    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = spawn_calendar_source(script_path, sender, cancelled.clone());

    let mut store = StateStore::new(FreshnessConfig::default());
    assert!(store.apply(
        StateUpdate::System(SystemUpdate::Calendar(Some(CalendarEvent {
            id: "gcalcli:1800000600:Design review".to_string(),
            title: "Design review".to_string(),
            location: Some("Room 2".to_string()),
            start_epoch: 1_800_000_600,
            end_epoch: Some(1_800_002_400),
            changed_at: 0,
        }))),
        1_800_000_000,
    ));

    let update = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("calendar health update");

    cancelled.store(true, Ordering::Relaxed);
    handle.join().expect("join calendar source");
    fs::remove_dir_all(&temp_dir).ok();

    assert!(store.apply(update, 1_800_000_030));
    assert_eq!(
        store
            .snapshot()
            .system
            .calendar
            .as_ref()
            .map(|event| event.title.as_str()),
        Some("Design review")
    );
    assert_eq!(
        store
            .snapshot()
            .system
            .source_health
            .get(&SourceId::Calendar),
        Some(&SourceHealth::Disconnected {
            message: "gcalcli unavailable".to_string(),
        })
    );
}

#[test]
fn parse_calendar_agenda_sorts_events_and_validates_range() {
    let request = CalendarMonthRequest::new(2027, 1).expect("month request");
    let agenda = parse_calendar_agenda_json(
        r#"{"healthy":true,"range_start":"2027-01-01","range_end":"2027-02-01","events":[{"id":"later","title":"Later","location":null,"calendar":"Work","start_epoch":1800439200,"end_epoch":1800442800,"all_day":false},{"id":"day","title":"Planning day","location":null,"calendar":"Personal","start_epoch":1800230400,"end_epoch":1800316800,"all_day":true}]}"#,
        request,
    )
    .expect("parse agenda");

    assert_eq!(agenda.year, 2027);
    assert_eq!(agenda.month, 1);
    assert_eq!(
        agenda
            .events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        vec!["day", "later"]
    );

    let error = parse_calendar_agenda_json(
        r#"{"healthy":true,"range_start":"2027-02-01","range_end":"2027-03-01","events":[]}"#,
        request,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("range does not match"));
}

#[test]
fn agenda_source_publishes_requested_month_without_touching_next_event() {
    let temp_dir = unique_temp_dir("calendar-agenda");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let script_path = temp_dir.join("next_event.sh");
    write_script(
        &script_path,
        r#"#!/usr/bin/env bash
printf '%s\n' '{"healthy":true,"range_start":"2027-01-01","range_end":"2027-02-01","events":[{"id":"review","title":"Review","location":"Room 2","calendar":"Work","start_epoch":1800000600,"end_epoch":1800002400,"all_day":false}]}'
"#,
    );

    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let (request_sender, handle) =
        spawn_calendar_agenda_source(script_path, sender, cancelled.clone());
    request_sender
        .send(CalendarMonthRequest::new(2027, 1).expect("request"))
        .expect("send month");

    let first = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("agenda update");
    let second = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("agenda health");
    cancelled.store(true, Ordering::Relaxed);
    drop(request_sender);
    handle.join().expect("join agenda source");
    fs::remove_dir_all(&temp_dir).ok();

    assert_eq!(
        first,
        StateUpdate::System(SystemUpdate::CalendarAgenda(Some(CalendarAgenda {
            year: 2027,
            month: 1,
            events: vec![CalendarAgendaEvent {
                id: "review".to_string(),
                title: "Review".to_string(),
                location: Some("Room 2".to_string()),
                calendar: Some("Work".to_string()),
                start_epoch: 1_800_000_600,
                end_epoch: 1_800_002_400,
                all_day: false,
            }],
        })))
    );
    assert_eq!(
        second,
        StateUpdate::Health {
            source: SourceId::CalendarAgenda,
            health: SourceHealth::Healthy,
        }
    );
}
