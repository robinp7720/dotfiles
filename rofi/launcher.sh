#!/usr/bin/env bash

set -euo pipefail

DOT_LAUNCHER_DEV_BIN="${DOT_LAUNCHER_DEV_BIN:-$HOME/.dotfiles/tools/launcher/target/release/dot-launcher}"
DOT_LAUNCHER_BIN="${DOT_LAUNCHER_BIN:-$HOME/.local/bin/dot-launcher}"

if [[ "${PREFER_DOT_LAUNCHER:-1}" == "1" ]]; then
  if [[ -x "$DOT_LAUNCHER_DEV_BIN" ]]; then
    exec "$DOT_LAUNCHER_DEV_BIN"
  fi

  if [[ -x "$DOT_LAUNCHER_BIN" ]]; then
    exec "$DOT_LAUNCHER_BIN"
  fi

  if command -v dot-launcher >/dev/null 2>&1; then
    exec "$(command -v dot-launcher)"
  fi
fi

ROFI_BIN="${ROFI_BIN:-$HOME/.local/bin/rofi}"
if [[ ! -x "$ROFI_BIN" ]]; then
  ROFI_BIN="$(command -v rofi)"
fi

exec "$ROFI_BIN" -show combi
