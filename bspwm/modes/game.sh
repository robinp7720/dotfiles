#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.config/bspwm/helpers/mode_common.sh"

load_environment
clear_mode

# Set the screen resolution and layout
run_screenlayout "$HOME/.screenlayout/game.sh"

# Start polybar
start_polybar

assign_desktops_if_present DP-2-2 "I" "II" "III" "IV" "V"

set_global_padding 40 0 0 0

start_background superpaper
