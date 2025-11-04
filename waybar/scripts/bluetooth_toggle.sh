#!/usr/bin/env bash

# Toggle Bluetooth controller power state.

set -euo pipefail

if ! command -v bluetoothctl >/dev/null 2>&1; then
  exit 0
fi

state="$(bluetoothctl show | awk '/Powered:/ {print $2}')"

if [[ "$state" == "yes" ]]; then
  bluetoothctl power off >/dev/null
else
  bluetoothctl power on >/dev/null
fi
