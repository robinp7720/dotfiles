#!/usr/bin/env bash
# Power monitors on or off through the active Wayland compositor.

set -euo pipefail

action="${1:-}"
case "$action" in
  on|off) ;;
  *)
    printf 'Usage: %s {on|off}\n' "$0" >&2
    exit 2
    ;;
esac

if command -v hyprctl >/dev/null 2>&1 && hyprctl monitors >/dev/null 2>&1; then
  hyprctl dispatch dpms "$action" >/dev/null
elif command -v niri >/dev/null 2>&1 && niri msg outputs >/dev/null 2>&1; then
  if [[ "$action" == "on" ]]; then
    niri msg action power-on-monitors >/dev/null
  else
    niri msg action power-off-monitors >/dev/null
  fi
else
  printf 'No supported Wayland compositor is available for DPMS control.\n' >&2
  exit 1
fi

if [[ "$action" == "on" ]] && command -v brightnessctl >/dev/null 2>&1; then
  brightnessctl -r >/dev/null 2>&1 || true
fi
