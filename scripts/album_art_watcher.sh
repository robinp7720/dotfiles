#!/usr/bin/env bash
# Watch playerctl metadata changes and keep Hyprlock album art fresh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NOW_PLAYING="$SCRIPT_DIR/now_playing.sh"

if ! command -v playerctl >/dev/null 2>&1; then
  exit 0
fi

refresh() {
  # --quiet updates cached art without printing track text
  "$NOW_PLAYING" --quiet
}

# initial refresh
refresh

# follow metadata changes (artUrl updates on track change)
playerctl metadata --follow -a 2>/dev/null | while read -r _; do
  refresh
done
