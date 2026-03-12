#!/usr/bin/env bash

set -euo pipefail

ROFI_BIN="${ROFI_BIN:-$HOME/.local/bin/rofi}"
ROFI_THEME="${HOME}/.config/rofi/quick_actions.rasi"
HEADPHONES_MAC="${HEADPHONES_MAC:-88:C9:E8:25:7B:04}"

if [[ ! -x "$ROFI_BIN" ]]; then
  ROFI_BIN="$(command -v rofi || true)"
fi

if [[ -z "${ROFI_BIN:-}" ]]; then
  printf 'rofi is not installed.\n' >&2
  exit 1
fi

if [[ ! -f "$ROFI_THEME" ]]; then
  ROFI_THEME="${HOME}/.dotfiles/rofi/quick_actions.rasi"
fi

cleanup() {
  if [[ -n "${tmp_dir:-}" && -d "${tmp_dir:-}" ]]; then
    rm -rf "$tmp_dir"
  fi
}

bluetooth_status() {
  if ! command -v bluetoothctl >/dev/null 2>&1; then
    printf 'Unavailable\n'
    return
  fi

  local state connected
  state="$(bluetoothctl show 2>/dev/null | awk '/Powered:/ {print $2}')"
  connected="$(bluetoothctl devices Connected 2>/dev/null | wc -l | tr -d ' ')"

  if [[ "$state" != "yes" ]]; then
    printf 'Off\n'
  elif [[ "$connected" -gt 0 ]]; then
    printf 'On · %s connected\n' "$connected"
  else
    printf 'On\n'
  fi
}

headphones_status() {
  if ! command -v bluetoothctl >/dev/null 2>&1; then
    printf 'Unavailable\n'
    return
  fi

  if bluetoothctl info "$HEADPHONES_MAC" 2>/dev/null | grep -q "Connected: yes"; then
    printf 'Connected\n'
  else
    printf 'Disconnected\n'
  fi
}

power_profile_status() {
  if ! command -v powerprofilesctl >/dev/null 2>&1; then
    printf 'Unavailable\n'
    return
  fi

  local profile
  profile="$(powerprofilesctl get 2>/dev/null || true)"
  case "$profile" in
    performance)
      printf 'Performance\n'
      ;;
    power-saver)
      printf 'Power Saver\n'
      ;;
    balanced)
      printf 'Balanced\n'
      ;;
    *)
      printf 'Unavailable\n'
      ;;
  esac
}

read_statuses() {
  tmp_dir="$(mktemp -d)"
  trap cleanup EXIT

  (
    bluetooth_status > "${tmp_dir}/bluetooth"
  ) &
  (
    headphones_status > "${tmp_dir}/headphones"
  ) &
  (
    power_profile_status > "${tmp_dir}/power"
  ) &
  wait

  BLUETOOTH_STATUS="$(<"${tmp_dir}/bluetooth")"
  HEADPHONES_STATUS="$(<"${tmp_dir}/headphones")"
  POWER_STATUS="$(<"${tmp_dir}/power")"
}

show_menu() {
  local selection
  local -a options=(
    "  Launcher             Open apps and commands"
    "  Bluetooth            ${BLUETOOTH_STATUS}"
    "  Headphones           ${HEADPHONES_STATUS}"
    "  Power profile        ${POWER_STATUS}"
    "󰌾  Lock now             Secure this session"
    "󰍛  Power menu           Suspend, reboot, shutdown"
  )

  selection="$(
    printf '%s\n' "${options[@]}" | "$ROFI_BIN" \
      -theme "$ROFI_THEME" \
      -dmenu \
      -i \
      -no-custom \
      -p "system" \
      -mesg "Fast one-shot actions for Super+B" \
      -l 6 \
      -format i
  )"

  printf '%s\n' "$selection"
}

run_toggle_and_refresh() {
  "$@" >/dev/null 2>&1 || true
  exec "$0"
}

read_statuses
choice="$(show_menu || true)"

case "$choice" in
  0)
    exec "$HOME/.config/rofi/launcher.sh"
    ;;
  1)
    run_toggle_and_refresh "$HOME/.dotfiles/waybar/scripts/bluetooth_toggle.sh"
    ;;
  2)
    run_toggle_and_refresh "$HOME/.dotfiles/waybar/scripts/headphones_toggle.sh"
    ;;
  3)
    run_toggle_and_refresh "$HOME/.dotfiles/waybar/scripts/power_profile_toggle.sh" --toggle
    ;;
  4)
    exec "$HOME/.dotfiles/scripts/hyprlock_with_art.sh"
    ;;
  5)
    exec "$HOME/.dotfiles/scripts/power_menu.sh"
    ;;
  *)
    exit 0
    ;;
esac
