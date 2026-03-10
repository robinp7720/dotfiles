#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.config/bspwm/helpers/mode_common.sh"

load_environment
"$HOME/.config/bspwm/modes/external_only.sh"
bspc desktop -l monocle

# Start steam in big picture mode
steam -start steam://open/bigpicture -fulldesktopres
