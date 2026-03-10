#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../helpers/mode_common.sh"

prepare_mode

bspc monitor primary -d "main"
run_screenlayout "$HOME/.screenlayout/game_seperate.sh"

set_global_padding 0 0 0 0
disable_display_power_saving
