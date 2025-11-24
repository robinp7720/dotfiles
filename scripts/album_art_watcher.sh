#!/usr/bin/env bash
# Watch playerctl metadata changes and keep Hyprlock album art fresh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UPDATE="$SCRIPT_DIR/update_album_art.sh"

if ! command -v playerctl >/dev/null 2>&1; then
  exit 0
fi

refresh() {
  "$UPDATE"
}

# initial refresh
refresh

# follow metadata changes (artUrl updates on track change)
playerctl metadata --follow 2>/dev/null | while read -r _; do
  refresh
done
