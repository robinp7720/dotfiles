use crate::{ActivityStatus, ContextCard, ContextTier};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextPresentation {
    pub icon_name: String,
    pub text: String,
    pub tier: ContextTier,
}

pub fn context_tier(card: &ContextCard) -> ContextTier {
    match card {
        ContextCard::Battery { tier, .. }
        | ContextCard::Calendar { tier, .. }
        | ContextCard::Timer { tier, .. } => *tier,
        ContextCard::Activity { .. } => ContextTier::Work,
        ContextCard::Media { .. } => ContextTier::Ambient,
        ContextCard::Urgent { .. } => ContextTier::Critical,
    }
}

pub fn context_presentation(card: &ContextCard, now_epoch: i64) -> ContextPresentation {
    ContextPresentation {
        icon_name: context_icon_name(card).to_string(),
        text: context_text(card, now_epoch),
        tier: context_tier(card),
    }
}

fn context_icon_name(card: &ContextCard) -> &'static str {
    match card {
        ContextCard::Battery { .. } => "battery-caution-symbolic",
        ContextCard::Calendar { .. } => "appointment-soon-symbolic",
        ContextCard::Timer { .. } => "alarm-symbolic",
        ContextCard::Activity { .. } => "system-run-symbolic",
        ContextCard::Media { .. } => "media-playback-start-symbolic",
        ContextCard::Urgent { .. } => "dialog-warning-symbolic",
    }
}

pub fn context_text(card: &ContextCard, now_epoch: i64) -> String {
    match card {
        ContextCard::Battery {
            percent, charging, ..
        } => {
            if *charging {
                format!("Battery {percent}% charging")
            } else {
                format!("Battery {percent}%")
            }
        }
        ContextCard::Calendar {
            title,
            location,
            start_epoch,
            ..
        } => {
            let event = location
                .as_ref()
                .map(|location| format!("{title} — {location}"))
                .unwrap_or_else(|| title.clone());
            format!("{event} · {}", relative_time(*start_epoch, now_epoch))
        }
        ContextCard::Timer {
            label,
            remaining_seconds,
            completed,
            ..
        } => {
            if *completed {
                format!("{label} done")
            } else {
                format!("{label} · {}", clock_duration(*remaining_seconds))
            }
        }
        ContextCard::Activity { label, status, .. } => {
            let status = match status {
                ActivityStatus::Running => "running",
                ActivityStatus::Succeeded => "finished",
                ActivityStatus::Failed => "failed",
            };
            format!("{label} · {status}")
        }
        ContextCard::Media {
            title,
            artist,
            player,
            ..
        } => match (title, artist) {
            (Some(title), Some(artist)) => format!("{title} - {artist}"),
            (Some(title), None) => title.clone(),
            _ => player.clone(),
        },
        ContextCard::Urgent {
            window_title,
            workspace,
            output,
            ..
        } => window_title
            .clone()
            .or_else(|| workspace.clone())
            .unwrap_or_else(|| format!("Urgent on {output}")),
    }
}

pub fn warning_text(card: &ContextCard) -> String {
    match card {
        ContextCard::Urgent {
            window_title,
            workspace,
            output,
            ..
        } => window_title
            .clone()
            .or_else(|| workspace.clone())
            .unwrap_or_else(|| format!("Urgent on {output}")),
        other => context_text(other, 0),
    }
}

fn relative_time(start_epoch: i64, now_epoch: i64) -> String {
    let remaining = start_epoch.saturating_sub(now_epoch);
    if remaining <= 0 {
        return "now".to_string();
    }

    if remaining < 60 {
        return "in <1m".to_string();
    }

    let minutes = (remaining + 59) / 60;
    if minutes < 120 {
        return format!("in {minutes}m");
    }

    let hours = (minutes + 59) / 60;
    if hours < 48 {
        return format!("in {hours}h");
    }

    let days = (hours + 23) / 24;
    format!("in {days}d")
}

fn clock_duration(seconds: u64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{ActivityStatus, ContextCard, ContextTier};

    use super::context_presentation;

    #[test]
    fn timer_presentation_uses_stable_clock_digits_and_icon() {
        let presentation = context_presentation(
            &ContextCard::Timer {
                id: "timer-1".to_string(),
                label: "Tea".to_string(),
                remaining_seconds: 62,
                target_epoch: Some(1_800_000_062),
                completed: false,
                tier: ContextTier::Imminent,
            },
            1_800_000_000,
        );

        assert_eq!(presentation.icon_name, "alarm-symbolic");
        assert_eq!(presentation.text, "Tea · 1:02");
    }

    #[test]
    fn calendar_presentation_includes_location_and_relative_time() {
        let presentation = context_presentation(
            &ContextCard::Calendar {
                id: "event-1".to_string(),
                title: "Review".to_string(),
                location: Some("Lab".to_string()),
                start_epoch: 1_800_000_600,
                tier: ContextTier::Imminent,
            },
            1_800_000_000,
        );

        assert_eq!(presentation.icon_name, "appointment-soon-symbolic");
        assert_eq!(presentation.text, "Review — Lab · in 10m");
    }

    #[test]
    fn activity_presentation_uses_human_status_text() {
        let presentation = context_presentation(
            &ContextCard::Activity {
                id: "build-1".to_string(),
                label: "Cargo test".to_string(),
                cwd: PathBuf::from("/tmp/project"),
                status: ActivityStatus::Succeeded,
                started_at: 1_800_000_000,
                finished_at: Some(1_800_000_010),
            },
            1_800_000_010,
        );

        assert_eq!(presentation.icon_name, "system-run-symbolic");
        assert_eq!(presentation.text, "Cargo test · finished");
    }
}
