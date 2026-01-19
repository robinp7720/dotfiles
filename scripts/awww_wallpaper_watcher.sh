#!/usr/bin/env bash
# Keep awww wallpapers applied on all outputs (reapply on new outputs).

set -euo pipefail

WALLPAPER="${1:-$HOME/.wallpaper.png}"
LOCK_FILE="${XDG_RUNTIME_DIR:-/tmp}/awww-wallpaper.lock"

if ! command -v awww >/dev/null 2>&1; then
  exit 0
fi

if [ ! -f "$WALLPAPER" ]; then
  exit 0
fi

# Ensure only one instance is running.
exec 9>"$LOCK_FILE"
if command -v flock >/dev/null 2>&1; then
  if ! flock -n 9; then
    exit 0
  fi
fi

wait_for_daemon() {
  local i
  for i in {1..20}; do
    if awww query >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  return 1
}

get_outputs_sorted() {
  niri msg outputs 2>/dev/null | awk -F'[()]' '/^Output /{print $2}' | sort -u
}

outputs_from_workspaces_line() {
  printf '%s\n' "$1" \
    | grep -oE 'output: Some\\(\"[^\"]+\"\\)' \
    | sed -E 's/^output: Some\\(\"|\"\\)$//g' \
    | sort -u
}

if ! wait_for_daemon; then
  exit 0
fi

if ! command -v niri >/dev/null 2>&1; then
  exit 0
fi

apply_wallpaper() {
  awww img "$WALLPAPER" >/dev/null 2>&1 || true
}

known_outputs="$(get_outputs_sorted || true)"
apply_wallpaper

niri msg event-stream 2>/dev/null | while IFS= read -r line; do
  case "$line" in
    Outputs\ changed:*)
      current_outputs="$(get_outputs_sorted || true)"
      ;;
    Workspaces\ changed:*)
      current_outputs="$(outputs_from_workspaces_line "$line")"
      if [ -z "$current_outputs" ]; then
        current_outputs="$(get_outputs_sorted || true)"
      fi
      ;;
    *)
      continue
      ;;
  esac

  if [ -n "$current_outputs" ]; then
    new_outputs="$(comm -13 <(printf '%s\n' "$known_outputs" | sort -u) <(printf '%s\n' "$current_outputs" | sort -u))"
    if [ -n "$new_outputs" ]; then
      apply_wallpaper
    fi
    known_outputs="$current_outputs"
  fi
done
