#!/usr/bin/env bash

# Connect/disconnect a specific Bluetooth headset from Waybar click.
# Device MAC is fixed; adjust if you change headphones.

set -euo pipefail

MAC="88:C9:E8:25:7B:04"

if ! command -v bluetoothctl >/dev/null 2>&1; then
  exit 0
fi

# Ensure controller is powered on so connect can succeed.
if ! bluetoothctl show | awk '/Powered:/ {print $2}' | grep -q yes; then
  bluetoothctl power on >/dev/null 2>&1 || true
  # Give the adapter a brief moment to settle.
  sleep 0.5
fi

if bluetoothctl info "$MAC" 2>/dev/null | grep -q "Connected: yes"; then
  bluetoothctl disconnect "$MAC" >/dev/null 2>&1
else
  # Start an agent so bluetoothctl can handle pairing/connecting if needed.
  bluetoothctl agent on >/dev/null 2>&1 || true
  bluetoothctl connect "$MAC" >/dev/null 2>&1
fi
