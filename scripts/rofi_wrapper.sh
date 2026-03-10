#!/usr/bin/env bash
set -euo pipefail

find_real_rofi() {
  if [[ -n "${ROFI_REAL_BIN:-}" ]]; then
    printf '%s\n' "$ROFI_REAL_BIN"
    return 0
  fi

  local self_path candidate
  self_path="$(readlink -f "$0" 2>/dev/null || printf '%s\n' "$0")"

  while IFS= read -r candidate; do
    [[ -n "$candidate" ]] || continue
    if [[ "$(readlink -f "$candidate" 2>/dev/null || printf '%s\n' "$candidate")" != "$self_path" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done < <(which -a rofi 2>/dev/null || true)

  if command -v rofi-wayland >/dev/null 2>&1; then
    command -v rofi-wayland
    return 0
  fi

  if [[ -x /usr/bin/rofi ]]; then
    printf '%s\n' "/usr/bin/rofi"
    return 0
  fi

  return 1
}

ROFI_REAL_BIN="$(find_real_rofi || true)"
if [[ -z "$ROFI_REAL_BIN" ]]; then
  printf 'Could not find a real rofi binary.\n' >&2
  exit 1
fi

if [[ -n "${NIRI_SOCKET-}" ]] || pgrep -x niri >/dev/null 2>&1; then
  exec "$ROFI_REAL_BIN" -theme "$HOME/.config/rofi/niri.rasi" "$@"
else
  exec "$ROFI_REAL_BIN" "$@"
fi
