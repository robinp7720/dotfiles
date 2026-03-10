#!/usr/bin/env bash
# Keep Hyprlock album art in sync with the shared now_playing logic.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NOW_PLAYING="$SCRIPT_DIR/now_playing.sh"

exec "$NOW_PLAYING" --quiet
