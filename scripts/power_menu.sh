#!/usr/bin/env bash
set -euo pipefail

# Simple power menu using rofi/rofi-wayland.
# Presents logout/shutdown/reboot options and executes the chosen action.

menu_prompt="Power"
options=(
  "  Shutdown"
  "  Reboot"
  "󰍃  Logout"
  "󰜺  Cancel"
)

choice=$(printf '%s\n' "${options[@]}" | rofi -dmenu -i -p "$menu_prompt")
# Extract the trailing label (text after the icon) so matching is stable even if fonts change.
choice=${choice#*  }

case "$choice" in
  Shutdown)
    systemctl poweroff
    ;;
  Reboot)
    systemctl reboot
    ;;
  Logout)
    if pgrep -x Hyprland >/dev/null 2>&1 && command -v hyprctl >/dev/null 2>&1; then
      hyprctl dispatch exit
    elif pgrep -x niri >/dev/null 2>&1 && command -v niri >/dev/null 2>&1; then
      # Use niri IPC to quit without showing the confirmation dialog.
      niri msg action quit --skip-confirmation
    elif pgrep -x bspwm >/dev/null 2>&1; then
      bspc quit
    elif command -v loginctl >/dev/null 2>&1 && [[ -n "${XDG_SESSION_ID:-}" ]]; then
      loginctl terminate-session "$XDG_SESSION_ID"
    else
      pkill -KILL -u "$USER"
    fi
    ;;
  *)
    ;;
esac
