#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

load_environment() {
  local env_file="$HOME/.config/environment"
  if [[ -f "$env_file" ]]; then
    # shellcheck source=/dev/null
    source "$env_file"
  fi
}

clear_mode() {
  "$SCRIPT_DIR/clear_mode.sh"
}

run_screenlayout() {
  local script="$1"
  "$script"
}

prepare_mode() {
  load_environment
  clear_mode
}

start_polybar() {
  "$SCRIPT_DIR/../../polybar/launch.sh" &
}

set_global_padding() {
  local top="$1"
  local bottom="$2"
  local left="$3"
  local right="$4"

  bspc config top_padding "$top"
  bspc config bottom_padding "$bottom"
  bspc config left_padding "$left"
  bspc config right_padding "$right"
}

start_background() {
  "$@" &
}

disable_display_power_saving() {
  if command -v xset >/dev/null 2>&1; then
    xset -dpms
    xset s off
  fi
}

monitor_exists() {
  bspc query -M --names | grep -Fxq -- "$1"
}

assign_desktops_if_present() {
  local monitor="$1"
  shift

  if monitor_exists "$monitor"; then
    bspc monitor "$monitor" -d "$@"
  fi
}

set_monitor_top_padding_if_present() {
  local monitor="$1"
  local top_padding="$2"

  if monitor_exists "$monitor"; then
    bspc config -m "$monitor" top_padding "$top_padding"
  fi
}
