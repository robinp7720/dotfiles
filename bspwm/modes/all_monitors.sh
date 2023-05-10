#! /bin/sh

source ~/.config/environment

~/.config/bspwm/helpers/clear_mode.sh

# Set the screen resolution and layout
~/.screenlayout/all_monitors.sh
#~/.screenlayout/using_splitter.sh

# Start polybar
~/.config/polybar/launch.sh &

bspc monitor DP-2 -d  "I" "II" "III" "IV" "V" 
bspc monitor DVI-D-0 -d "VI" "VII" "VIII" "IX" "X"
bspc monitor HDMI-0 -d "Secondary I" "Secondary II"

bspc config top_padding     40
bspc config bottom_padding  0
bspc config left_padding    0
bspc config right_padding   0

bspc config -m HDMI-1 top_padding 0

superpaper &
