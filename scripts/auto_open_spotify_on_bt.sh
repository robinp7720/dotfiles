#!/usr/bin/env bash
# Launch Spotify when a Bluetooth audio sink (headphones/earbuds) connects.

set -euo pipefail

log_event() {
  if command -v logger >/dev/null 2>&1; then
    logger -t auto-spotify "$*"
  fi
}

open_spotify() {
  if ! pgrep -x spotify >/dev/null 2>&1; then
    spotify >/dev/null 2>&1 &
    return 0
  fi
  return 1
}

start_playback() {
  if ! command -v playerctl >/dev/null 2>&1; then
    return 0
  fi

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

handle_bt_state_change() {
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
}

if ! command -v pactl >/dev/null 2>&1 || ! command -v spotify >/dev/null 2>&1; then
  exit 0
fi

prev_has_bt=false

# Trigger once on startup in case headphones are already connected.
log_event "startup: checking bluetooth sink state"
handle_bt_state_change

# Listen for new/changed sinks; reconnect if pactl exits cleanly after an audio daemon restart.
while true; do
  while read -r line; do
    case "$line" in
      *"on sink"*|*"on server"*)
        log_event "event received: $line"
        handle_bt_state_change
        ;;
    esac
  done < <(pactl subscribe 2>/dev/null)

  log_event "pactl subscribe ended; reconnecting in 1s"
  sleep 1
done
