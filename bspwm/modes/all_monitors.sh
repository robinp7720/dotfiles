#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../helpers/mode_common.sh"

prepare_mode

run_screenlayout "$HOME/.screenlayout/all_monitors.sh"

start_polybar
set_global_padding 40 0 0 0

assign_desktops_if_present DP-2 "I" "II" "III" "IV" "V"
assign_desktops_if_present DVI-D-0 "VI" "VII" "VIII" "IX" "X"

while IFS= read -r m; do
  case "$m" in
    DP-2|DVI-D-0) ;;
    *) bspc monitor "$m" -d "${m//-/_}_1" ;;
  esac
done < <(bspc query -M --names)

while IFS= read -r m; do
  case "$m" in
    DP-2|DVI-D-0|DP-0) set_monitor_top_padding_if_present "$m" 40 ;;
    *) set_monitor_top_padding_if_present "$m" 0 ;;
  esac
done < <(bspc query -M --names)

start_background nitrogen --restore
