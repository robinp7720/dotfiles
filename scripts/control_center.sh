#!/usr/bin/env bash

set -euo pipefail

EWW_BIN="${EWW_BIN:-$(command -v eww || true)}"
EWW_CONFIG="${HOME}/.config/eww"
WINDOW_NAME="control_center"
STATE_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/control_center-open"

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
    eww_cmd daemon
    sleep 0.3
  fi
}

refresh() {
  local vars=(
    cc_clock
    cc_date
    cc_next_event
    cc_media
    cc_bluetooth
    cc_headphones
    cc_power_profile
    cc_battery
    cc_network
    agenda_item_0
    agenda_item_1
    agenda_item_2
    album_art_path
  )

  ensure_daemon
  for var_name in "${vars[@]}"; do
    eww_cmd poll "$var_name" >/dev/null 2>&1 || true
  done
}

is_open() {
  eww_cmd active-windows 2>/dev/null | awk -F': ' '{print $2}' | grep -qx "$WINDOW_NAME" ||
    [[ -f "$STATE_FILE" ]]
}

open_center() {
  ensure_daemon
  refresh
  if eww_cmd open "$WINDOW_NAME"; then
    mkdir -p "$(dirname "$STATE_FILE")"
    : > "$STATE_FILE"
  fi
}

close_center() {
  ensure_daemon
  eww_cmd close "$WINDOW_NAME" >/dev/null 2>&1 || true
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
