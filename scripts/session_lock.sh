#!/usr/bin/env bash
# Lock the current Wayland session through the shared Hyprlock surface.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HYPRLOCK_BIN="${HYPRLOCK_BIN-$(command -v hyprlock || true)}"
SWAYLOCK_BIN="${SWAYLOCK_BIN-$(command -v swaylock || true)}"
VIGIL_BIN="${VIGIL_BIN-$(command -v vigil || true)}"
RENDER_CONFIG_BIN="${RENDER_CONFIG_BIN:-$SCRIPT_DIR/render_hyprlock_config.sh}"
ALBUM_ART_UPDATE_BIN="${ALBUM_ART_UPDATE_BIN:-$SCRIPT_DIR/update_album_art.sh}"
RUNTIME_CONFIG=""

locker_running() {
  pgrep -u "$(id -u)" -x hyprlock >/dev/null 2>&1 ||
    pgrep -u "$(id -u)" -x swaylock >/dev/null 2>&1
}

close_transient_surfaces() {
  if [[ -n "$VIGIL_BIN" ]]; then
    "$VIGIL_BIN" control-center close >/dev/null 2>&1 || true
  fi
}

if locker_running; then
  exit 0
fi

close_transient_surfaces

# Cached personal context must never delay acquisition of the security surface.
"$ALBUM_ART_UPDATE_BIN" >/dev/null 2>&1 &

if [[ -n "$HYPRLOCK_BIN" ]]; then
  RUNTIME_CONFIG="$($RENDER_CONFIG_BIN 2>/dev/null || true)"
  if [[ -z "$RUNTIME_CONFIG" ]]; then
    RUNTIME_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/hypr/hyprlock.conf"
  fi

  if "$HYPRLOCK_BIN" \
    --config "$RUNTIME_CONFIG" \
    --immediate-render \
    --no-fade-in \
    "$@"; then
    exit 0
  fi
fi

if [[ -n "$SWAYLOCK_BIN" ]]; then
  exec "$SWAYLOCK_BIN" --daemonize
fi

printf 'No lock command is available for the current session.\n' >&2
exit 1
