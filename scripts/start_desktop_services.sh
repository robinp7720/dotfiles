#!/usr/bin/env bash
set -euo pipefail

# Run once from the compositor so every service gets the new session's sockets.
: "${WAYLAND_DISPLAY:?WAYLAND_DISPLAY is required}"
case "${1:-}" in
  hyprland)
    : "${HYPRLAND_INSTANCE_SIGNATURE:?HYPRLAND_INSTANCE_SIGNATURE is required}"
    export NIRI_SOCKET=
    ;;
  niri)
    : "${NIRI_SOCKET:?NIRI_SOCKET is required}"
    export HYPRLAND_INSTANCE_SIGNATURE=
    ;;
  *)
    printf 'Usage: %s hyprland|niri\n' "$0" >&2
    exit 1
    ;;
esac

# Keep D-Bus activation's full environment, but explicitly import session values
# into systemd. Empty compositor variables also clear the previous session.
dbus-update-activation-environment --all
vars=(
  WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE NIRI_SOCKET
  XDG_RUNTIME_DIR XDG_SESSION_DESKTOP XDG_SESSION_TYPE DBUS_SESSION_BUS_ADDRESS
  DISPLAY SSH_AUTH_SOCK
)
imported=()
for name in "${vars[@]}"; do
  if [[ -v "$name" ]]; then
    imported+=("$name")
  fi
done
systemctl --user import-environment "${imported[@]}"

# Wait only for the curtain's first opaque frame, never for desktop readiness.
# The helper watches this compositor and reveals independently after startup.
# Missing helpers, reduced motion and display failures cannot block login.
if [[ "$1" == hyprland ]] && command -v shell-greeter >/dev/null 2>&1; then
  read -r -t 2 transition_ready < <(shell-greeter --desktop-reveal) || true
fi

services=(
  vigil.service
  awww-daemon.service
  awww-wallpaper.service
  hypridle.service
  cliphist.service
)
# Retries during a compositor restart can exhaust systemd's start limit. The
# fresh session environment is installed now, so allow a new startup attempt.
systemctl --user reset-failed "${services[@]}"
systemctl --user restart "${services[@]}"
