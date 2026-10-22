#!/usr/bin/env bash
# Switch the hyprsunset night light. Luma and Vigil both call this so the
# warm temperature and the on/off test live in one place.
set -euo pipefail

NIGHT_KELVIN="${NIGHT_LIGHT_KELVIN:-4000}"
# hyprsunset reports 6000K when no filter is applied.
NEUTRAL_KELVIN=6000

current_kelvin() {
  hyprctl hyprsunset temperature 2>/dev/null | tr -dc '0-9'
}

is_on() {
  local kelvin
  kelvin="$(current_kelvin)"
  [[ -n "$kelvin" && "$kelvin" -lt "$NEUTRAL_KELVIN" ]]
}

turn_on() {
  hyprctl hyprsunset temperature "$NIGHT_KELVIN" >/dev/null
}

turn_off() {
  hyprctl hyprsunset identity >/dev/null
}

case "${1:-toggle}" in
  on)
    turn_on
    ;;
  off)
    turn_off
    ;;
  toggle)
    if is_on; then
      turn_off
    else
      turn_on
    fi
    ;;
  status)
    if is_on; then
      echo on
    else
      echo off
    fi
    ;;
  *)
    echo "usage: $(basename "$0") [on|off|toggle|status]" >&2
    exit 2
    ;;
esac
