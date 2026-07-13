use crate::{ContextCard, ContextTier};

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

pub fn context_text(card: &ContextCard) -> String {
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
            title, location, ..
        } => location
            .as_ref()
            .map(|location| format!("{title} - {location}"))
            .unwrap_or_else(|| title.clone()),
        ContextCard::Timer {
            label,
            remaining_seconds,
            completed,
            ..
        } => {
            if *completed {
                format!("{label} done")
            } else if *remaining_seconds >= 60 {
                format!("{label} {}m", remaining_seconds / 60)
            } else {
                format!("{label} {remaining_seconds}s")
            }
        }
        ContextCard::Activity { label, status, .. } => format!("{label} ({status:?})"),
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
        other => context_text(other),
    }
}
