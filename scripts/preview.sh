#!/usr/bin/env bash
# File and directory previews for the shell's FZF picker.
set -euo pipefail

target="${1:-}"
if [[ -d "$target" ]]; then
  exec eza -lbF --git --icons --header -- "$target"
elif [[ -f "$target" ]]; then
  exec bat --color=always --style=numbers --line-range=:500 -- "$target"
fi
