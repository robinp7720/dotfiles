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
const PREFERRED_PLAYER_FAMILIES: &[&str] = &[
    "spotify",
    "ncspot",
    "spotifyd",
    "tidal-hifi",
    "tidal",
    "cider",
    "rhythmbox",
    "lollypop",
    "amberol",
    "cmus",
    "mpd",
    "mpv",
    "celluloid",
    "vlc",
];
const BROWSER_PLAYER_FAMILIES: &[&str] = &[
    "firefox",
    "chromium",
    "chrome",
    "google-chrome",
    "brave",
    "vivaldi",
    "microsoft-edge",
    "plasma-browser-integration",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayerEvent {
    player: String,
    status: PlaybackStatus,
    title: Option<String>,
    artist: Option<String>,
    art_url: Option<String>,
}

fn parse_playerctl_event(line: &str) -> Result<PlayerEvent> {
    let mut fields = line.splitn(5, FIELD_SEPARATOR);
    let player = normalize_field(fields.next().unwrap_or_default())
        .context("playerctl line did not include a player name")?;
    let status = parse_playback_status(fields.next().unwrap_or_default());
    let title = normalize_field(fields.next().unwrap_or_default());
    let artist = normalize_field(fields.next().unwrap_or_default());
    let art_url = normalize_field(fields.next().unwrap_or_default());

    Ok(PlayerEvent {
        player,
        status,
        title,
        artist,
        art_url,
    })
}

fn apply_player_event(
    players: &mut BTreeMap<String, MediaState>,
    mut event: PlayerEvent,
) -> Option<MediaState> {
    if matches!(event.status, PlaybackStatus::Stopped) {
        players.remove(&event.player);
    } else {
        if event.art_url.is_none()
            && let Some(previous) = players.get(&event.player)
            && same_track(previous, &event)
        {
            event.art_url = previous.art_url.clone();
        }
        players.insert(
            event.player.clone(),
            MediaState {
                player: event.player,
                status: event.status,
                title: event.title,
                artist: event.artist,
                art_url: event.art_url,
                changed_at: 0,
            },
        );
    }

    preferred_player(players)
}

fn same_track(previous: &MediaState, event: &PlayerEvent) -> bool {
    event.title.is_some()
        && previous.title == event.title
        && (event.artist.is_none() || previous.artist == event.artist)
}

fn preferred_player(players: &BTreeMap<String, MediaState>) -> Option<MediaState> {
    players
        .values()
        .min_by_key(|state| player_selection_key(state))
        .cloned()
}

fn player_selection_key(state: &MediaState) -> (u8, usize, String) {
    (
        playback_priority(&state.status),
        player_priority(&state.player),
        state.player.to_ascii_lowercase(),
    )
}

fn playback_priority(status: &PlaybackStatus) -> u8 {
    match status {
        PlaybackStatus::Playing => 0,
        PlaybackStatus::Paused => 1,
        PlaybackStatus::Stopped => 2,
    }
}

fn player_priority(player: &str) -> usize {
    if let Some(index) = PREFERRED_PLAYER_FAMILIES
        .iter()
        .position(|family| player_matches_family(player, family))
    {
        return index;
    }

    if BROWSER_PLAYER_FAMILIES
        .iter()
        .any(|family| player_matches_family(player, family))
    {
        return PREFERRED_PLAYER_FAMILIES.len() + 1;
    }

    PREFERRED_PLAYER_FAMILIES.len()
}

fn player_matches_family(player: &str, family: &str) -> bool {
    player.eq_ignore_ascii_case(family)
        || player
            .get(..family.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(family))
            && player.as_bytes().get(family.len()) == Some(&b'.')
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
        "{{{{playerName}}}}{FIELD_SEPARATOR}{{{{status}}}}{FIELD_SEPARATOR}{{{{title}}}}{FIELD_SEPARATOR}{{{{artist}}}}{FIELD_SEPARATOR}{{{{mpris:artUrl}}}}"
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

    use super::{FIELD_SEPARATOR, apply_player_event, parse_playerctl_event, player_priority};
    use crate::{MediaState, PlaybackStatus};

    fn event_line(player: &str, status: &str, title: &str, artist: &str) -> String {
        format!(
            "{player}{FIELD_SEPARATOR}{status}{FIELD_SEPARATOR}{title}{FIELD_SEPARATOR}{artist}{FIELD_SEPARATOR}"
        )
    }

    fn event_line_with_art(
        player: &str,
        status: &str,
        title: &str,
        artist: &str,
        art_url: &str,
    ) -> String {
        format!(
            "{player}{FIELD_SEPARATOR}{status}{FIELD_SEPARATOR}{title}{FIELD_SEPARATOR}{artist}{FIELD_SEPARATOR}{art_url}"
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
        assert_eq!(event.art_url, None);
    }

    #[test]
    fn playerctl_metadata_preserves_local_and_remote_artwork_uris() {
        for art_url in [
            "file:///tmp/album%20art.jpg",
            "https://cdn.example.test/artwork.webp",
        ] {
            let event = parse_playerctl_event(&event_line_with_art(
                "spotify", "Playing", "Track", "Artist", art_url,
            ))
            .unwrap();

            assert_eq!(event.art_url.as_deref(), Some(art_url));
        }
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
                art_url: None,
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
                art_url: None,
                changed_at: 0,
            })
        );
    }

    #[test]
    fn preferred_players_win_equal_status_ties_over_browsers() {
        for events in [
            [
                event_line("firefox", "Playing", "Video", "Browser"),
                event_line("spotify", "Playing", "Track", "Artist"),
            ],
            [
                event_line("spotify", "Playing", "Track", "Artist"),
                event_line("firefox", "Playing", "Video", "Browser"),
            ],
        ] {
            let mut players = BTreeMap::new();
            let mut selected = None;
            for line in events {
                selected = apply_player_event(
                    &mut players,
                    parse_playerctl_event(&line).expect("parse player event"),
                );
            }

            assert_eq!(selected.unwrap().player, "spotify");
        }
    }

    #[test]
    fn selection_falls_back_when_the_preferred_player_stops() {
        let mut players = BTreeMap::new();
        for line in [
            event_line("firefox", "Playing", "Video", "Browser"),
            event_line("spotify", "Playing", "Track", "Artist"),
        ] {
            let _ = apply_player_event(
                &mut players,
                parse_playerctl_event(&line).expect("parse player event"),
            );
        }

        let selected = apply_player_event(
            &mut players,
            parse_playerctl_event(&event_line("spotify", "Stopped", "", ""))
                .expect("parse stopped event"),
        );

        assert_eq!(selected.unwrap().player, "firefox");
    }

    #[test]
    fn player_families_match_instance_suffixes_case_insensitively() {
        assert!(player_priority("Spotify.instance_2") < player_priority("firefox.instance_1"));
        assert!(player_priority("kdeconnect") < player_priority("firefox.instance_1"));
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

    #[test]
    fn partial_updates_keep_artwork_for_the_same_track() {
        let mut players = BTreeMap::new();
        let with_art = parse_playerctl_event(&event_line_with_art(
            "firefox",
            "Playing",
            "Track",
            "Artist",
            "file:///tmp/art.jpg",
        ))
        .unwrap();
        let without_art =
            parse_playerctl_event(&event_line("firefox", "Playing", "Track", "Artist")).unwrap();

        let _ = apply_player_event(&mut players, with_art);
        let selected = apply_player_event(&mut players, without_art).unwrap();

        assert_eq!(selected.art_url.as_deref(), Some("file:///tmp/art.jpg"));
    }

    #[test]
    fn a_new_track_does_not_inherit_previous_artwork() {
        let mut players = BTreeMap::new();
        let with_art = parse_playerctl_event(&event_line_with_art(
            "firefox",
            "Playing",
            "First track",
            "Artist",
            "file:///tmp/first.jpg",
        ))
        .unwrap();
        let next_track =
            parse_playerctl_event(&event_line("firefox", "Playing", "Second track", "Artist"))
                .unwrap();

        let _ = apply_player_event(&mut players, with_art);
        let selected = apply_player_event(&mut players, next_track).unwrap();

        assert_eq!(selected.art_url, None);
    }
}
