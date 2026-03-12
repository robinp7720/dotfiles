#!/usr/bin/env bash

set -euo pipefail

HEADPHONES_MAC="${HEADPHONES_MAC:-88:C9:E8:25:7B:04}"
HEADPHONES_ALIAS="${HEADPHONES_ALIAS:-Headphones}"

title_case() {
  local value="$1"

  value="${value//-/ }"
  read -r -a words <<<"$value"
  for i in "${!words[@]}"; do
    local word="${words[i]}"
    words[i]="${word^}"
  done

  printf '%s\n' "${words[*]}"
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
  if [[ -z "$profile" ]]; then
    printf 'Unavailable\n'
    return
  fi

  title_case "$profile"
}

battery_status() {
  local battery_dir
  battery_dir="$(find /sys/class/power_supply -maxdepth 1 -type d -name 'BAT*' | head -n1)"

  if [[ -z "$battery_dir" ]]; then
    printf 'Desktop\n'
    return
  fi

  local capacity status
  capacity="$(<"$battery_dir/capacity")"
  status="$(<"$battery_dir/status")"

  case "$status" in
    Charging)
      printf '%s%% · Charging\n' "$capacity"
      ;;
    Full)
      printf '%s%% · Full\n' "$capacity"
      ;;
    *)
      printf '%s%%\n' "$capacity"
      ;;
  esac
}

network_status() {
  if command -v nmcli >/dev/null 2>&1; then
    local info type name
    info="$(nmcli -t -f TYPE,STATE,CONNECTION device status 2>/dev/null | awk -F: '$2=="connected" && ($1=="wifi" || $1=="ethernet") {print $1 ":" $3; exit}')"

    if [[ -n "$info" ]]; then
      type="${info%%:*}"
      name="${info#*:}"
      if [[ "$type" == "wifi" ]]; then
        printf 'Wi-Fi · %s\n' "$name"
      else
        printf 'Ethernet\n'
      fi
      return
    fi
  fi

  if ip route get 1.1.1.1 >/dev/null 2>&1; then
    printf 'Connected\n'
  else
    printf 'Offline\n'
  fi
}

media_status() {
  local value
  value="$("$HOME/.dotfiles/scripts/now_playing.sh" 2>/dev/null || true)"

  if [[ -n "$value" ]]; then
    printf '%s\n' "$value"
  else
    printf 'Nothing playing\n'
  fi
}

clock_status() {
  date +"%H:%M"
}

date_status() {
  date +"%A, %B %-d"
}

case "${1:-}" in
  bluetooth)
    bluetooth_status
    ;;
  headphones)
    headphones_status
    ;;
  power)
    power_profile_status
    ;;
  battery)
    battery_status
    ;;
  network)
    network_status
    ;;
  media)
    media_status
    ;;
  clock)
    clock_status
    ;;
  date)
    date_status
    ;;
  *)
    printf 'Usage: %s {bluetooth|headphones|power|battery|network|media|clock|date}\n' "$0" >&2
    exit 1
    ;;
esac
