#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}" 2>/dev/null || printf '%s\n' "${BASH_SOURCE[0]}")"
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
. "$SCRIPT_DIR/session_common.sh"

kitty_bin="${KITTY_REAL_BIN:-/usr/bin/kitty}"

# Use translucent Kitty on Hyprland, opaque on niri (no blur support).
if is_hyprland_session; then
  exec "$kitty_bin" "$@"
fi

if is_niri_session; then
  exec "$kitty_bin" --override background_opacity=1.0 "$@"
fi

exec "$kitty_bin" "$@"
