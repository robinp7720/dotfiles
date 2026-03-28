#!/usr/bin/env bash

set -euo pipefail

if [[ "${PREFER_DOT_LAUNCHER:-1}" == "1" ]] && command -v dot-launcher >/dev/null 2>&1; then
  exec dot-launcher
fi

ROFI_BIN="${ROFI_BIN:-$HOME/.local/bin/rofi}"
if [[ ! -x "$ROFI_BIN" ]]; then
  ROFI_BIN="$(command -v rofi)"
fi

exec "$ROFI_BIN" -show combi
