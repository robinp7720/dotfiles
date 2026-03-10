#!/usr/bin/env bash

set -euo pipefail

load_environment() {
  local env_file="$HOME/.config/environment"
  if [[ -f "$env_file" ]]; then
    # shellcheck source=/dev/null
    source "$env_file"
  fi
}

clear_mode() {
  "$HOME/.config/bspwm/helpers/clear_mode.sh"
}

run_screenlayout() {
  local script="$1"
  "$script"
}

start_polybar() {
  "$HOME/.config/polybar/launch.sh" &
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
