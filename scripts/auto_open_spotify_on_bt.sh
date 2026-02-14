#!/usr/bin/env bash
# Launch Spotify when a Bluetooth audio sink (headphones/earbuds) connects.

set -euo pipefail

log_event() {
  logger -t auto-spotify "$*"
}

open_spotify() {
  if ! pgrep -x spotify >/dev/null 2>&1; then
    spotify >/dev/null 2>&1 &
    return 0
  fi
  return 1
}

start_playback() {
  # Wait a few seconds for Spotify's MPRIS interface to come up, then play.
  for _ in {1..15}; do
    if playerctl --player=spotify status >/dev/null 2>&1; then
      playerctl --player=spotify play >/dev/null 2>&1 && return 0
    fi
    sleep 1
  done
  return 1
}

has_bt_sink() {
  pactl list short sinks | grep -qE "bluez_(sink|output)"
}

prev_has_bt=false
spotify_was_launched=0

# Trigger once on startup in case headphones are already connected.
if has_bt_sink; then
  prev_has_bt=true
  log_event "startup: bluetooth sink already connected"
  if open_spotify; then
    start_playback || true
  fi
else
  log_event "startup: no bluetooth sink"
fi

# Listen for new/changed sinks; fire when a Bluetooth sink appears.
pactl subscribe | while read -r line; do
  case "$line" in
    *"on sink"*|*"on server"*)
      log_event "event received: $line"
      if has_bt_sink; then
        log_event "bluetooth sink detected (prev=$prev_has_bt)"
        if [ "$prev_has_bt" = false ]; then
          log_event "launching spotify"
          if open_spotify; then
            start_playback || true
          fi
        fi
        prev_has_bt=true
      else
        log_event "no bluetooth sink (prev=$prev_has_bt)"
        prev_has_bt=false
      fi
      ;;
  esac
done
