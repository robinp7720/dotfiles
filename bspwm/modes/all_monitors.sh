#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.config/bspwm/helpers/mode_common.sh"

load_environment
clear_mode

# Set the screen resolution and layout
run_screenlayout "$HOME/.screenlayout/all_monitors.sh"
#~/.screenlayout/using_splitter.sh

# Start polybar
start_polybar

set_global_padding 40 0 0 0

# Mirror Hyprland workspace split: main DP-2 (1-5), side DVI-D-0 (6-10).
assign_desktops_if_present DP-2 "I" "II" "III" "IV" "V"
assign_desktops_if_present DVI-D-0 "VI" "VII" "VIII" "IX" "X"

# Give any remaining connected monitors a single desktop to keep bspwm happy.
while IFS= read -r m; do
  case "$m" in
    DP-2|DVI-D-0) ;; # already configured above
    *)
      bspc monitor "$m" -d "${m//-/_}_1"
      ;;
  esac
done < <(bspc query -M --names)

# Give 40px padding to monitors that host bars.
while IFS= read -r m; do
  case "$m" in
    DP-2|DVI-D-0|DP-0) bspc config -m "$m" top_padding 40 ;;
    *) bspc config -m "$m" top_padding 0 ;;
  esac
done < <(bspc query -M --names)

start_background nitrogen --restore
#superpaper &
