#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../helpers/mode_common.sh"

prepare_mode

run_screenlayout "$HOME/.screenlayout/game.sh"
start_polybar

assign_desktops_if_present DP-2-2 "I" "II" "III" "IV" "V"

set_global_padding 40 0 0 0

start_background superpaper
