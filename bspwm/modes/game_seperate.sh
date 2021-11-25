#! /bin/sh

source ~/.config/environment

bspc monitor primary -d "main"

~/.config/bspwm/helpers/clear_mode.sh

# Set the screen resolution and layout
~/.screenlayout/game_seperate.sh

# Start polybar
~/.config/polybar/launch.sh &

bspc config top_padding     0
bspc config bottom_padding  0
bspc config left_padding    0
bspc config right_padding   0

superpaper &
