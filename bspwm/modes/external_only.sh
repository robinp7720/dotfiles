#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.config/bspwm/helpers/mode_common.sh"

load_environment

bspc monitor primary -d "main"
clear_mode

# Set the screen resolution and layout
run_screenlayout "$HOME/.screenlayout/game_seperate.sh"

# Start polybar
#~/.config/polybar/launch.sh &

set_global_padding 0 0 0 0

#superpaper &

xset -dpms
xset s off
