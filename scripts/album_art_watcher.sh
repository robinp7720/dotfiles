#!/usr/bin/env bash
# Watch playerctl metadata changes and keep Hyprlock album art fresh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NOW_PLAYING="${NOW_PLAYING_BIN:-$SCRIPT_DIR/now_playing.sh}"
RETRY_SECONDS="${ALBUM_ART_WATCHER_RETRY_SECONDS:-2}"

if ! command -v playerctl >/dev/null 2>&1; then
  exit 0
fi

refresh() {
  # --quiet updates cached art without printing track text
  "$NOW_PLAYING" --quiet || true
}

# Reconnect when no player exists yet or the MPRIS stream disappears. This
# keeps album art fresh after players start, stop, or restart mid-session.
while true; do
  refresh
  playerctl metadata --follow -a 2>/dev/null | while read -r _; do
    refresh
  done || true
  sleep "$RETRY_SECONDS"
done
