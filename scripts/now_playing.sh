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
action_mode=""

while [[ $# -gt 0 ]]; do
  case "${1:-}" in
    --quiet)
      output_mode="quiet"
      ;;
    --toggle)
      action_mode="play-pause"
      ;;
    --next)
      action_mode="next"
      ;;
    --previous)
      action_mode="previous"
      ;;
  esac
  shift
done

chosen_status=""
chosen_artist=""
chosen_title=""
chosen_art_url=""
chosen_player=""
metadata_separator=$'\x1f'
source="playerctl"

# Vigil already tracks the active player for the bar; ask it first so the lock
# screen, media keys and bar agree. Fails when Vigil is not running.
read_from_vigil() {
  local json fields
  command -v vigil >/dev/null 2>&1 || return 1
  command -v jq >/dev/null 2>&1 || return 1
  json="$(vigil media get 2>/dev/null)" || return 1
  fields="$(jq -r --arg sep "$metadata_separator" \
    '.media // empty | [.player, .status, (.artist // ""), (.title // ""), (.art_url // "")] | join($sep)' \
    <<< "$json")" || return 1
  if [[ -n "$fields" ]]; then
    IFS="$metadata_separator" read -r chosen_player chosen_status chosen_artist chosen_title chosen_art_url <<< "$fields"
  fi
}

read_from_playerctl() {
  local players player info status artist title art_url

  # Stay silent if playerctl is unavailable.
  command -v playerctl >/dev/null 2>&1 || return 0

  # Iterate players and pick one that has title/artist; prefer a Playing player.
  players=$(playerctl -l 2>/dev/null || true)
  [[ -n "$players" ]] || return 0

  while IFS= read -r player; do
    # Use a control-character separator so ordinary metadata punctuation cannot
    # shift fields and corrupt the displayed title or album-art URL.
    info=$(playerctl --player="$player" metadata --format "{{status}}${metadata_separator}{{artist}}${metadata_separator}{{title}}${metadata_separator}{{mpris:artUrl}}" 2>/dev/null || true)
    IFS="$metadata_separator" read -r status artist title art_url <<< "$info"

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
}

if read_from_vigil; then
  source="vigil"
else
  read_from_playerctl
fi

update_art() {
  local status="$1" art_url="$2"

  # Clear art when not actively playing to avoid stale images.
  if [[ "$status" != "Playing" || -z "$art_url" ]]; then
    rm -f "$OUT"
    return
  fi

  case "$art_url" in
    file://*)
      local art_path
      art_path="$(decode_uri_path "${art_url#file://}")"
      cp "$art_path" "$OUT" 2>/dev/null || rm -f "$OUT"
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

decode_uri_path() {
  local encoded="$1"
  local decoded=""
  local prefix=""

  while [[ "$encoded" == *%* ]]; do
    prefix="${encoded%%\%*}"
    decoded+="$prefix"
    encoded="${encoded#*\%}"

    if [[ "$encoded" =~ ^([[:xdigit:]]{2})(.*)$ ]]; then
      printf -v prefix '%b' "\\x${BASH_REMATCH[1]}"
      decoded+="$prefix"
      encoded="${BASH_REMATCH[2]}"
    else
      decoded+="%"
    fi
  done

  printf '%s%s' "$decoded" "$encoded"
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

if [[ -n "$action_mode" ]]; then
  if [[ "$source" == "vigil" ]]; then
    vigil media "$action_mode" >/dev/null 2>&1 || true
  else
    playerctl --player="$chosen_player" "$action_mode" >/dev/null 2>&1 || true
  fi
  exit 0
fi

update_art "$chosen_status" "$chosen_art_url"

icon="🎵"
if [[ "$chosen_status" == "Paused" ]]; then
  icon="⏸"
fi

escape_markup() {
  local s="$1"
  s="${s//&/&amp;}"
  s="${s//</&lt;}"
  s="${s//>/&gt;}"
  echo "$s"
}

if [[ "$output_mode" == "text" ]]; then
  safe_artist=$(escape_markup "$chosen_artist")
  safe_title=$(escape_markup "$chosen_title")
  if [[ -n "$chosen_artist" ]]; then
    echo "$icon $safe_artist — $safe_title"
  else
    echo "$icon $safe_title"
  fi
fi
