#!/usr/bin/env bash

set -euo pipefail

ROFI_BIN="${ROFI_BIN:-$HOME/.local/bin/rofi}"
if [[ ! -x "$ROFI_BIN" ]]; then
  ROFI_BIN="$(command -v rofi)"
fi

exec "$ROFI_BIN" -show combi
