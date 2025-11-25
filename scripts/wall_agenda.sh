#!/usr/bin/env bash
# Overlay next calendar event onto the current wallpaper and apply via swww.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NEXT_EVENT="$SCRIPT_DIR/next_event.sh"

OUT="/tmp/wall_agenda.png"

# Prefer explicit base via env; otherwise reuse current swww image; fallback to a conventional path.
BASE="${WALLPAPER_BASE:-}"
if [[ -z "$BASE" ]]; then
  BASE="$(swww query 2>/dev/null | sed -n 's/.*image: //p' | head -n1 || true)"
fi
if [[ -z "$BASE" ]]; then
  BASE="$HOME/Pictures/Wallpapers/current.jpg"
fi

# Soft-fail if prerequisites or base image are missing.
if ! command -v swww >/dev/null 2>&1; then
  exit 0
fi
if ! command -v convert >/dev/null 2>&1; then
  exit 0
fi
if [[ ! -f "$BASE" ]]; then
  exit 0
fi

# Ensure swww daemon is running.
swww query >/dev/null 2>&1 || swww init

event_text="$("$NEXT_EVENT" 2>/dev/null || true)"
if [[ -z "$event_text" ]]; then
  event_text="No upcoming events"
fi

# Draw outlined text for legibility.
convert "$BASE" \
  -gravity southeast \
  -fill white \
  -stroke '#00000099' -strokewidth 2 \
  -font "JetBrainsMono Nerd Font" \
  -pointsize 28 \
  -annotate +80+80 "$event_text" \
  -stroke none \
  -annotate +80+80 "$event_text" \
  "$OUT"

swww img "$OUT" --transition-type any --transition-duration 0.6
