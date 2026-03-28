#!/usr/bin/env bash
set -euo pipefail

find_rofi_bin() {
  local candidate="${ROFI_BIN:-$HOME/.local/bin/rofi}"

  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  command -v rofi || true
}

escape_markup() {
  local value="${1:-}"

  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"

  printf '%s' "$value"
}

render_row() {
  local icon="$1"
  local title="$2"
  local description="$3"

  printf '<span weight="700" size="110%%">%s  %s</span>&#10;<span size="90%%">%s</span>' \
    "$icon" \
    "$(escape_markup "$title")" \
    "$(escape_markup "$description")"
}

ROFI_BIN="$(find_rofi_bin)"
if [[ -z "${ROFI_BIN:-}" ]]; then
  printf 'rofi is not installed.\n' >&2
  exit 1
fi

ROFI_THEME="${ROFI_THEME:-$HOME/.config/rofi/quick_actions.rasi}"
if [[ ! -f "$ROFI_THEME" ]]; then
  ROFI_THEME="$HOME/.dotfiles/rofi/quick_actions.rasi"
fi

menu() {
  local prompt="$1"
  local message="$2"
  shift 2

  printf '%s\n' "$@" | "$ROFI_BIN" \
    -theme "$ROFI_THEME" \
    -dmenu \
    -i \
    -markup-rows \
    -no-custom \
    -p "$prompt" \
    -mesg "$message" \
    -l "$#" \
    -format i
}

confirm() {
  local action="$1"
  local answer_index

  answer_index="$(
    menu "confirm" "This action runs immediately after confirmation" \
      "$(render_row "󰜺" "Cancel" "Keep the current session untouched")" \
      "$(render_row "󰄬" "Confirm ${action}" "Proceed with ${action} now")"
  )" || true

  [[ "$answer_index" == "1" ]]
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
  "$(render_row "󰍁" "Lock" "Blank the screen and keep the session running")"
  "$(render_row "󰤄" "Suspend" "Lock first, then suspend the machine")"
  "$(render_row "󰍃" "Logout" "Close the current desktop session")"
  "$(render_row "" "Reboot" "Restart the system")"
  "$(render_row "" "Shutdown" "Power off the system")"
  "$(render_row "󰜺" "Cancel" "Close this menu")"
)

choice="$(menu "session" "Session controls with confirmation for destructive actions" "${options[@]}")" || true

case "$choice" in
  0)
    lock_session
    ;;
  1)
    if confirm "suspend"; then
      lock_session || true
      sleep 1
      systemctl suspend
    fi
    ;;
  2)
    confirm "logout" && logout_session
    ;;
  3)
    confirm "reboot" && systemctl reboot
    ;;
  4)
    confirm "shutdown" && systemctl poweroff
    ;;
  *)
    ;;
esac
