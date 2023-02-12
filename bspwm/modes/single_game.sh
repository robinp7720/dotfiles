#! /bin/sh

source ~/.config/environment

~/.config/bspwm/helpers/clear_mode.sh

bspc config top_padding     40
bspc config bottom_padding  0
bspc config left_padding    0
bspc config right_padding   0

# Set the screen resolution and layout
~/.screenlayout/single.sh

# Start polybar
~/.config/polybar/launch.sh &

bspc monitor DP-2 -d  "I" "II" "III" "IV" "V" 

superpaper &
