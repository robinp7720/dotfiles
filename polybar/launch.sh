#!/usr/bin/env bash

set -euo pipefail

LOG_FILE="${XDG_RUNTIME_DIR:-/tmp}/polybar_main.log"
CONFIG_FILE="$HOME/.dotfiles/polybar/config.ini"

if ! command -v polybar >/dev/null 2>&1; then
  exit 0
fi

if command -v polybar-msg >/dev/null 2>&1; then
  polybar-msg cmd quit >/dev/null 2>&1 || true
fi

for _ in {1..40}; do
  if ! pgrep -x polybar >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

printf '%s\n' "---" >> "$LOG_FILE"
polybar main -c "$CONFIG_FILE" >> "$LOG_FILE" 2>&1 & disown

printf 'Polybar launched using %s\n' "$CONFIG_FILE"
