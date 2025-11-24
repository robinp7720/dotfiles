#!/usr/bin/env bash
# Print the current track for Hyprlock and keep the album art in sync with the
# same player we display. Stays quiet if nothing useful is playing. Pass
# --quiet to update art only (no text output) for compatibility with the
# album_art_watcher.

set -euo pipefail

CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/hyprlock"
OUT="$CACHE_DIR/album.jpg"
mkdir -p "$CACHE_DIR"

output_mode="text"
if [[ ${1:-} == "--quiet" ]]; then
  output_mode="quiet"
fi

# Exit silently if playerctl is unavailable.
if ! command -v playerctl >/dev/null 2>&1; then
  exit 0
fi

# Iterate players and pick one that has title/artist; prefer a Playing player.
players=$(playerctl -l 2>/dev/null || true)
if [[ -z "$players" ]]; then
  rm -f "$OUT"
  exit 0
fi

chosen_status=""
chosen_artist=""
chosen_title=""
chosen_art_url=""
chosen_player=""

while IFS= read -r player; do
  # metadata --format is cheaper than separate calls
  info=$(playerctl --player="$player" metadata --format '{{status}}|{{artist}}|{{title}}|{{mpris:artUrl}}' 2>/dev/null || true)
  status=${info%%|*}
  rest=${info#*|}
  artist=${rest%%|*}
  rest=${rest#*|}
  title=${rest%%|*}
  art_url=${rest#*|}

  # Skip if no title (nothing meaningful to show)
  if [[ -z "$title" ]]; then
    continue
  fi

  # Keep the first with status Playing; otherwise remember the first paused/stopped with metadata.
  if [[ "$status" == "Playing" ]]; then
    chosen_status=$status
    chosen_artist=$artist
    chosen_title=$title
    chosen_art_url=$art_url
    chosen_player=$player
    break
  fi

  if [[ -z "$chosen_title" ]]; then
    chosen_status=$status
    chosen_artist=$artist
    chosen_title=$title
    chosen_art_url=$art_url
    chosen_player=$player
  fi
done <<< "$players"

update_art() {
  local status="$1" art_url="$2"

  # Clear art when not actively playing to avoid stale images.
  if [[ "$status" != "Playing" || -z "$art_url" ]]; then
    rm -f "$OUT"
    return
  fi

  case "$art_url" in
    file://*)
      cp "${art_url#file://}" "$OUT" 2>/dev/null || rm -f "$OUT"
      ;;
    http://*|https://*)
      tmp=$(mktemp "$CACHE_DIR/artXXXX")
      if curl -fsSL --max-time 5 "$art_url" -o "$tmp"; then
        mv "$tmp" "$OUT"
      else
        rm -f "$tmp" "$OUT"
      fi
      ;;
    *)
      if [[ -f "$art_url" ]]; then
        cp "$art_url" "$OUT" 2>/dev/null || rm -f "$OUT"
      else
        rm -f "$OUT"
      fi
      ;;
  esac
}

# Nothing to show
if [[ -z "$chosen_title" ]]; then
  rm -f "$OUT"
  exit 0
fi

# Treat Stopped as no output to avoid stale info
if [[ "$chosen_status" == "Stopped" ]]; then
  rm -f "$OUT"
  exit 0
fi

update_art "$chosen_status" "$chosen_art_url"

icon="🎵"
if [[ "$chosen_status" == "Paused" ]]; then
  icon="⏸"
fi

if [[ "$output_mode" == "text" ]]; then
  if [[ -n "$chosen_artist" ]]; then
    echo "$icon $chosen_artist — $chosen_title"
  else
    echo "$icon $chosen_title"
  fi
fi
