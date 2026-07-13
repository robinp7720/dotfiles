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

use crate::{MediaState, PlaybackStatus, SourceHealth, SourceId, StateUpdate, SystemUpdate};

const FIELD_SEPARATOR: char = '\u{1f}';
const PLAYERCTL_EVENT_TIMEOUT: Duration = Duration::from_millis(250);
const MEDIA_RESTART_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayerEvent {
    player: String,
    status: PlaybackStatus,
    title: Option<String>,
    artist: Option<String>,
}

fn parse_playerctl_event(line: &str) -> Result<PlayerEvent> {
    let mut fields = line.splitn(4, FIELD_SEPARATOR);
    let player = normalize_field(fields.next().unwrap_or_default())
        .context("playerctl line did not include a player name")?;
    let status = parse_playback_status(fields.next().unwrap_or_default());
    let title = normalize_field(fields.next().unwrap_or_default());
    let artist = normalize_field(fields.next().unwrap_or_default());

    Ok(PlayerEvent {
        player,
        status,
        title,
        artist,
    })
}

fn apply_player_event(
    players: &mut BTreeMap<String, MediaState>,
    event: PlayerEvent,
) -> Option<MediaState> {
    if matches!(event.status, PlaybackStatus::Stopped) {
        players.remove(&event.player);
    } else {
        players.insert(
            event.player.clone(),
            MediaState {
                player: event.player,
                status: event.status,
                title: event.title,
                artist: event.artist,
                changed_at: 0,
            },
        );
    }

    preferred_player(players)
}

fn preferred_player(players: &BTreeMap<String, MediaState>) -> Option<MediaState> {
    players
        .values()
        .find(|state| matches!(state.status, PlaybackStatus::Playing))
        .cloned()
        .or_else(|| {
            players
                .values()
                .find(|state| matches!(state.status, PlaybackStatus::Paused))
                .cloned()
        })
}

fn normalize_field(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_playback_status(value: &str) -> PlaybackStatus {
    match value.trim() {
        "Playing" => PlaybackStatus::Playing,
        "Paused" => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    }
}

fn playerctl_format() -> String {
    format!(
        "{{{{playerName}}}}{FIELD_SEPARATOR}{{{{status}}}}{FIELD_SEPARATOR}{{{{title}}}}{FIELD_SEPARATOR}{{{{artist}}}}"
    )
}

fn publish_media_state(
    sender: &Sender<StateUpdate>,
    cancelled: &Arc<AtomicBool>,
    value: Option<MediaState>,
) {
    if sender
        .send(StateUpdate::System(SystemUpdate::Media(value)))
        .is_err()
    {
        cancelled.store(true, Ordering::Relaxed);
    }
}

fn read_initial_players() -> Result<BTreeMap<String, MediaState>> {
    let output = Command::new("playerctl")
        .args(["--all-players", "metadata", "--format"])
        .arg(playerctl_format())
        .output()
        .context("failed to execute playerctl metadata")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("No players found")
            || stderr.contains("No player could handle this command")
        {
            return Ok(BTreeMap::new());
        }
        bail!("playerctl metadata failed: {stderr}");
    }

    let stdout =
        String::from_utf8(output.stdout).context("playerctl metadata output was not UTF-8")?;
    let mut players = BTreeMap::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let event = parse_playerctl_event(line)?;
        let _ = apply_player_event(&mut players, event);
    }
    Ok(players)
}

fn spawn_playerctl_bridge() -> Result<(Child, mpsc::Receiver<Result<String>>)> {
    let mut child = Command::new("playerctl")
        .args(["--follow", "--all-players", "metadata", "--format"])
        .arg(playerctl_format())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute playerctl --follow metadata")?;
    let stdout = child
        .stdout
        .take()
        .context("playerctl --follow metadata did not provide stdout")?;
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

fn run_media_worker(sender: &Sender<StateUpdate>, cancelled: &Arc<AtomicBool>) -> Result<bool> {
    let mut players = read_initial_players()?;
    publish_media_state(sender, cancelled, preferred_player(&players));

    let (mut child, receiver) = spawn_playerctl_bridge()?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            kill_child(&mut child);
            return Ok(false);
        }

        match receiver.recv_timeout(PLAYERCTL_EVENT_TIMEOUT) {
            Ok(Ok(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                let event = parse_playerctl_event(&line)?;
                let selection = apply_player_event(&mut players, event);
                publish_media_state(sender, cancelled, selection);
            }
            Ok(Err(error)) => {
                kill_child(&mut child);
                return Err(error);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = child
                    .try_wait()
                    .context("failed to poll playerctl --follow")?
                {
                    if status.success() {
                        return Ok(true);
                    }
                    return Err(anyhow!("playerctl --follow metadata exited with {status}"));
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

pub fn spawn_media_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    super::SourceSupervisor::spawn(cancelled.clone(), MEDIA_RESTART_DELAY, move || {
        match run_media_worker(&sender, &cancelled) {
            Ok(healthy) => Ok(healthy),
            Err(error) => {
                let _ = sender.send(StateUpdate::Health {
                    source: SourceId::Media,
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
    use std::collections::BTreeMap;

    use super::{FIELD_SEPARATOR, apply_player_event, parse_playerctl_event};
    use crate::{MediaState, PlaybackStatus};

    fn event_line(player: &str, status: &str, title: &str, artist: &str) -> String {
        format!(
            "{player}{FIELD_SEPARATOR}{status}{FIELD_SEPARATOR}{title}{FIELD_SEPARATOR}{artist}"
        )
    }

    #[test]
    fn playerctl_metadata_preserves_user_visible_separators() {
        let event = parse_playerctl_event(&event_line(
            "spotify",
            "Playing",
            "Track | Live",
            "Artist - Guest",
        ))
        .unwrap();

        assert_eq!(event.title.as_deref(), Some("Track | Live"));
        assert_eq!(event.artist.as_deref(), Some("Artist - Guest"));
    }

    #[test]
    fn playing_players_win_over_paused_players() {
        let mut players = BTreeMap::new();
        let paused = parse_playerctl_event(&event_line(
            "spotify",
            "Paused",
            "Paused track",
            "Paused artist",
        ))
        .unwrap();
        let playing = parse_playerctl_event(&event_line(
            "firefox",
            "Playing",
            "Playing track",
            "Playing artist",
        ))
        .unwrap();

        let first = apply_player_event(&mut players, paused);
        let second = apply_player_event(&mut players, playing);

        assert_eq!(
            first,
            Some(MediaState {
                player: "spotify".to_string(),
                status: PlaybackStatus::Paused,
                title: Some("Paused track".to_string()),
                artist: Some("Paused artist".to_string()),
                changed_at: 0,
            })
        );
        assert_eq!(
            second,
            Some(MediaState {
                player: "firefox".to_string(),
                status: PlaybackStatus::Playing,
                title: Some("Playing track".to_string()),
                artist: Some("Playing artist".to_string()),
                changed_at: 0,
            })
        );
    }

    #[test]
    fn stopped_players_are_removed_from_the_current_snapshot() {
        let mut players = BTreeMap::new();
        let _ = apply_player_event(
            &mut players,
            parse_playerctl_event(&event_line("spotify", "Playing", "Track", "Artist")).unwrap(),
        );

        let selected = apply_player_event(
            &mut players,
            parse_playerctl_event(&event_line("spotify", "Stopped", "", "")).unwrap(),
        );

        assert_eq!(selected, None);
        assert!(players.is_empty());
    }
}
