#!/usr/bin/env bash
# Lock the current session with compositor-aware fallbacks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/session_common.sh"

HYPRLOCK_BIN="${HYPRLOCK_BIN:-$(command -v hyprlock || true)}"

run_hyprlock() {
  if [[ -z "${HYPRLOCK_BIN:-}" ]]; then
    return 1
  fi

  # Avoid stacking multiple lock screens if a lock is already active.
  if pgrep -u "$UID" -x hyprlock >/dev/null 2>&1; then
    return 0
  fi

  "$SCRIPT_DIR/update_album_art.sh"
  exec "$HYPRLOCK_BIN" "$@"
}

if is_hyprland_session; then
  run_hyprlock "$@" && exit 0
fi

if lock_current_logind_session; then
  exit 0
fi

run_hyprlock "$@" && exit 0

printf 'No lock command is available for the current session.\n' >&2
exit 1
