#!/usr/bin/env bash
set -euo pipefail

# Default to /usr/bin/rofi if not set
ROFI_REAL_BIN="${ROFI_REAL_BIN:-/usr/bin/rofi}"

# Check for Niri session
if [[ -n "${NIRI_SOCKET-}" ]] || pgrep -x niri >/dev/null 2>&1; then
  # Use the Niri-specific theme (no transparency)
  exec "$ROFI_REAL_BIN" -theme "$HOME/.config/rofi/niri.rasi" "$@"
else
  # Default behavior
  exec "$ROFI_REAL_BIN" "$@"
fi
