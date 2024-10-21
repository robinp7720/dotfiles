#! /bin/sh

source ~/.config/environment

~/.config/bspwm/modes/external_only.sh

bspc desktop -l monocle

# Start steam in big picture mode
steam -start steam://open/bigpicture -fulldesktopres
