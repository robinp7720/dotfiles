#!/usr/bin/env bash
set -euo pipefail

kitty_bin="${KITTY_REAL_BIN:-/usr/bin/kitty}"

# Use translucent Kitty on Hyprland, opaque on niri (no blur support).
if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE-}" ]]; then
  exec "$kitty_bin" "$@"
fi

if [[ -n "${NIRI_SOCKET-}" ]] || pgrep -x niri >/dev/null 2>&1; then
  exec "$kitty_bin" --override background_opacity=1.0 "$@"
fi

exec "$kitty_bin" "$@"
