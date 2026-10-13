#!/usr/bin/env bash

set -euo pipefail

layout_name="${1:?usage: apply_layout.sh <layout.lua>}"
source_layout="$HOME/.config/hypr/monitor_layouts/$layout_name"
target_layout="$HOME/.config/hypr/monitors.lua"

if [[ ! -f "$source_layout" ]]; then
  printf 'Missing Hypr monitor layout: %s\n' "$source_layout" >&2
  exit 1
fi

# Replace the watched module atomically, then apply it immediately.
temporary_layout="$(mktemp "${target_layout}.XXXXXX")"
trap 'rm -f -- "$temporary_layout"' EXIT
cat -- "$source_layout" > "$temporary_layout"
chmod 644 "$temporary_layout"
mv -- "$temporary_layout" "$target_layout"
hyprctl reload
