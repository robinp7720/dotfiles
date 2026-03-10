#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
layout_name="${1:?usage: apply_layout.sh <layout.conf>}"
source_layout="$HOME/.config/hypr/monitor_layouts/$layout_name"
target_layout="$HOME/.config/hypr/monitors.conf"

if [[ ! -f "$source_layout" ]]; then
  printf 'Missing Hypr monitor layout: %s\n' "$source_layout" >&2
  exit 1
fi

cp -- "$source_layout" "$target_layout"
