#!/usr/bin/env bash

set -euo pipefail

EWW_BIN="${EWW_BIN:-$(command -v eww || true)}"
EWW_CONFIG="${HOME}/.config/eww"
WINDOW_NAME="control_center"
STATE_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/control_center-open"
REFRESH_PID_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/control_center-refresh.pid"
STATUS_SCRIPT="$HOME/.config/eww/scripts/control_center_status.sh"

if [[ -z "$EWW_BIN" ]]; then
  printf 'eww is not installed.\n' >&2
  exit 1
fi

eww_cmd() {
  "$EWW_BIN" -c "$EWW_CONFIG" "$@"
}

ensure_daemon() {
  if ! eww_cmd ping >/dev/null 2>&1; then
    rm -f "$STATE_FILE"
    stop_refresh_loop
    eww_cmd daemon
    sleep 0.3
  fi
}

read_value() {
  "$@" 2>/dev/null || true
}

update_var() {
  local name="$1"
  local value="$2"

  eww_cmd update "$name=$value" >/dev/null 2>&1 || true
}

refresh() {
  local polled_vars=(
    agenda_item_0
    agenda_item_1
    agenda_item_2
    album_art_path
  )

  ensure_daemon
  update_var cc_clock "$(read_value "$STATUS_SCRIPT" clock)"
  update_var cc_date "$(read_value "$STATUS_SCRIPT" date)"
  update_var cc_next_event "$(read_value "$HOME/.dotfiles/scripts/next_event.sh")"
  update_var cc_media "$(read_value "$STATUS_SCRIPT" media)"
  update_var cc_bluetooth "$(read_value "$STATUS_SCRIPT" bluetooth)"
  update_var cc_headphones "$(read_value "$STATUS_SCRIPT" headphones)"
  update_var cc_power_profile "$(read_value "$STATUS_SCRIPT" power)"
  update_var cc_battery "$(read_value "$STATUS_SCRIPT" battery)"
  update_var cc_network "$(read_value "$STATUS_SCRIPT" network)"

  for var_name in "${polled_vars[@]}"; do
    eww_cmd poll "$var_name" >/dev/null 2>&1 || true
  done
}

stop_refresh_loop() {
  if [[ -f "$REFRESH_PID_FILE" ]]; then
    local pid
    pid="$(<"$REFRESH_PID_FILE")"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
    fi
    rm -f "$REFRESH_PID_FILE"
  fi
}

is_open() {
  eww_cmd active-windows 2>/dev/null | awk -F': ' '{print $2}' | grep -qx "$WINDOW_NAME" ||
    [[ -f "$STATE_FILE" ]]
}

start_refresh_loop() {
  stop_refresh_loop

  (
    while true; do
      if ! is_open; then
        break
      fi

      refresh
      sleep 5
    done
  ) >/dev/null 2>&1 &

  mkdir -p "$(dirname "$REFRESH_PID_FILE")"
  printf '%s\n' "$!" > "$REFRESH_PID_FILE"
}

focused_screen() {
  local screen=""

  if command -v hyprctl >/dev/null 2>&1; then
    screen="$(
      hyprctl activeworkspace -j 2>/dev/null | python3 -c '
import json
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

monitor = data.get("monitor") if isinstance(data, dict) else ""
if isinstance(monitor, str):
    print(monitor)
' 2>/dev/null || true
    )"
  fi

  if [[ -z "$screen" ]] && command -v niri >/dev/null 2>&1; then
    screen="$(
      niri msg focused-output --json 2>/dev/null | python3 -c '
import json
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

name = data.get("name") if isinstance(data, dict) else ""
if isinstance(name, str):
    print(name)
' 2>/dev/null || true
    )"
  fi

  printf '%s\n' "$screen"
}

open_center() {
  local screen=""

  ensure_daemon
  refresh
  screen="$(focused_screen)"

  if [[ -n "$screen" ]]; then
    eww_cmd open "$WINDOW_NAME" --screen "$screen" || eww_cmd open "$WINDOW_NAME"
  else
    eww_cmd open "$WINDOW_NAME"
  fi

  if is_open; then
    mkdir -p "$(dirname "$STATE_FILE")"
    : > "$STATE_FILE"
    start_refresh_loop
  fi
}

close_center() {
  stop_refresh_loop
  if eww_cmd ping >/dev/null 2>&1; then
    eww_cmd close "$WINDOW_NAME" >/dev/null 2>&1 || true
  fi
  rm -f "$STATE_FILE"
}

toggle_center() {
  ensure_daemon
  if is_open; then
    close_center
  else
    open_center
  fi
}

case "${1:-toggle}" in
  toggle)
    toggle_center
    ;;
  open)
    open_center
    ;;
  close)
    close_center
    ;;
  refresh)
    refresh
    ;;
  bluetooth-toggle)
    "$HOME/.dotfiles/waybar/scripts/bluetooth_toggle.sh"
    refresh
    ;;
  headphones-toggle)
    "$HOME/.dotfiles/waybar/scripts/headphones_toggle.sh"
    refresh
    ;;
  power-profile-cycle)
    "$HOME/.dotfiles/waybar/scripts/power_profile_toggle.sh" --toggle >/dev/null
    refresh
    ;;
  launcher)
    close_center
    exec "$HOME/.config/rofi/launcher.sh"
    ;;
  session-menu)
    close_center
    exec "$HOME/.dotfiles/scripts/power_menu.sh"
    ;;
  lock)
    close_center
    exec "$HOME/.dotfiles/scripts/hyprlock_with_art.sh"
    ;;
  *)
    printf 'Usage: %s {toggle|open|close|refresh|bluetooth-toggle|headphones-toggle|power-profile-cycle|launcher|session-menu|lock}\n' "$0" >&2
    exit 1
    ;;
esac
