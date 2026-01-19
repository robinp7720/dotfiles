#!/usr/bin/env bash
set -euo pipefail

config_path="${XDG_CONFIG_HOME:-$HOME/.config}/waybar/config"
style_path="${XDG_CONFIG_HOME:-$HOME/.config}/waybar/style.css"

launch_waybar_cmd="waybar -c \"$config_path\" -s \"$style_path\""

pkill -x waybar >/dev/null 2>&1 || true
pkill -x dunst >/dev/null 2>&1 || true

if command -v hyprctl >/dev/null 2>&1; then
  if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE-}" ]] || pgrep -x Hyprland >/dev/null 2>&1; then
    hyprctl dispatch exec "$launch_waybar_cmd" >/dev/null 2>&1 || true
    exit 0
  fi
fi

if command -v niri >/dev/null 2>&1; then
  if pgrep -x niri >/dev/null 2>&1; then
    niri msg action spawn-sh -- "$launch_waybar_cmd" >/dev/null 2>&1 || true
    exit 0
  fi
fi

nohup waybar -c "$config_path" -s "$style_path" >/dev/null 2>&1 &
