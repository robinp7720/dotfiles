#!/usr/bin/env bash
# Launch Spotify when a Bluetooth audio sink (headphones/earbuds) connects.

set -euo pipefail

open_spotify() {
  if ! pgrep -x spotify >/dev/null 2>&1; then
    spotify >/dev/null 2>&1 &
  fi
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

# Trigger once on startup in case headphones are already connected.
if has_bt_sink; then
  open_spotify
  start_playback || true
fi

# Listen for new/changed sinks; fire when a Bluetooth sink appears.
pactl subscribe | while read -r line; do
  case "$line" in
    *"on sink"*|*"on server"*)
      if has_bt_sink; then
        open_spotify
        start_playback || true
      fi
      ;;
  esac
done
