#!/usr/bin/env bash
set -euo pipefail

menu() {
  local prompt="$1"
  shift

  printf '%s\n' "$@" | "$HOME/.local/bin/rofi" -dmenu -i -p "$prompt"
}

confirm() {
  local action="$1"
  local answer

  answer=$(menu "Confirm" "󰜺  No" "󰄬  Yes, ${action}")
  answer=${answer#*  }
  [[ "$answer" == "Yes, ${action}" ]]
}

lock_session() {
  if pgrep -x Hyprland >/dev/null 2>&1 && command -v hyprlock >/dev/null 2>&1; then
    "$HOME/.dotfiles/scripts/hyprlock_with_art.sh" &
    disown
    return 0
  fi

  if command -v loginctl >/dev/null 2>&1 && [[ -n "${XDG_SESSION_ID:-}" ]]; then
    loginctl lock-session "$XDG_SESSION_ID"
    return 0
  fi

  if command -v hyprlock >/dev/null 2>&1; then
    hyprlock
    return 0
  fi

  printf 'No lock command is available for the current session.\n' >&2
  exit 1
}

logout_session() {
  if pgrep -x Hyprland >/dev/null 2>&1 && command -v hyprctl >/dev/null 2>&1; then
    hyprctl dispatch exit
  elif pgrep -x niri >/dev/null 2>&1 && command -v niri >/dev/null 2>&1; then
    niri msg action quit --skip-confirmation
  elif pgrep -x bspwm >/dev/null 2>&1 && command -v bspc >/dev/null 2>&1; then
    bspc quit
  elif command -v loginctl >/dev/null 2>&1 && [[ -n "${XDG_SESSION_ID:-}" ]]; then
    loginctl terminate-session "$XDG_SESSION_ID"
  else
    printf 'No safe logout method found for the current session.\n' >&2
    exit 1
  fi
}

options=(
  "󰍁  Lock"
  "󰤄  Suspend"
  "󰍃  Logout"
  "  Reboot"
  "  Shutdown"
  "󰜺  Cancel"
)

choice=$(menu "Session" "${options[@]}")
choice=${choice#*  }

case "$choice" in
  Lock)
    lock_session
    ;;
  Suspend)
    lock_session || true
    sleep 1
    systemctl suspend
    ;;
  Logout)
    confirm "logout" && logout_session
    ;;
  Reboot)
    confirm "reboot" && systemctl reboot
    ;;
  Shutdown)
    confirm "shutdown" && systemctl poweroff
    ;;
  *)
    ;;
esac
