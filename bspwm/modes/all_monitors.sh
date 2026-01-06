#!/usr/bin/env bash

source ~/.config/environment

~/.config/bspwm/helpers/clear_mode.sh

# Set the screen resolution and layout
~/.screenlayout/all_monitors.sh
#~/.screenlayout/using_splitter.sh

# Start polybar
~/.config/polybar/launch.sh &

bspc config top_padding     40   # room for the main polybar
bspc config bottom_padding  0
bspc config left_padding    0
bspc config right_padding   0

# Mirror Hyprland workspace split: main DP-2 (1-5), side DVI-D-0 (6-10).
if bspc query -M --names | grep -q '^DP-2$'; then
  bspc monitor DP-2 -d "I" "II" "III" "IV" "V"
fi

if bspc query -M --names | grep -q '^DVI-D-0$'; then
  bspc monitor DVI-D-0 -d "VI" "VII" "VIII" "IX" "X"
fi

# Give any remaining connected monitors a single desktop to keep bspwm happy.
for m in $(bspc query -M --names); do
  case "$m" in
    DP-2|DVI-D-0) ;; # already configured above
    *)
      bspc monitor "$m" -d "${m//-/_}_1"
      ;;
  esac
done

# Give 40px padding to monitors that host bars.
for m in $(bspc query -M --names); do
  case "$m" in
    DP-2|DVI-D-0|DP-0) bspc config -m "$m" top_padding 40 ;;
    *) bspc config -m "$m" top_padding 0 ;;
  esac
done

nitrogen --restore &
#superpaper &
