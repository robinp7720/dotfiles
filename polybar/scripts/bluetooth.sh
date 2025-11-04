#!/usr/bin/env bash
set -euo pipefail

icon_on=""
icon_off=""

show_output() {
  echo "${1}"
  exit 0
}

if ! command -v bluetoothctl >/dev/null 2>&1; then
  show_output "${icon_off}"
fi

power_state="$(bluetoothctl show 2>/dev/null | awk '/Powered:/{print $2}')"

if [[ "${power_state:-}" != "yes" ]]; then
  show_output "${icon_off} Off"
fi

connected_devices="$(bluetoothctl devices Connected 2>/dev/null | cut -d' ' -f2-)"

if [[ -z "${connected_devices}" ]]; then
  show_output "${icon_on}"
fi

# Collapse multiple connected devices onto a single, comma-separated line.
connected_summary="$(sed '/^\s*$/d' <<< "${connected_devices}" | paste -sd ', ' -)"

show_output "${icon_on} ${connected_summary}"
