#!/usr/bin/env bash

set -euo pipefail

CONFIG="${WAYBAR_CONFIG:-$HOME/.config/waybar/config}"
STYLE="${WAYBAR_STYLE:-$HOME/.config/waybar/style.css}"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}/dotfiles-waybar"
RUNTIME_CONFIG="$RUNTIME_DIR/config.json"

if [[ "${1:-}" == "--replace" ]]; then
  pkill -x waybar >/dev/null 2>&1 || true
  shift
fi

mkdir -p "$RUNTIME_DIR"

python3 - "$CONFIG" "$RUNTIME_CONFIG" <<'PY'
import json
import os
import sys
from pathlib import Path

source = Path(sys.argv[1]).expanduser()
target = Path(sys.argv[2])

with source.open(encoding="utf-8") as handle:
    config = json.load(handle)

desktop = os.environ.get("XDG_CURRENT_DESKTOP", "").lower()
session = os.environ.get("DESKTOP_SESSION", "").lower()

is_hyprland = bool(os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")) or "hyprland" in {
    desktop,
    session,
}
is_niri = bool(os.environ.get("NIRI_SOCKET")) or "niri" in {desktop, session}

modules_left = list(config.get("modules-left", []))
if is_hyprland and not is_niri:
    modules_left = [module for module in modules_left if not module.startswith("niri/")]
elif is_niri and not is_hyprland:
    modules_left = [
        module
        for module in modules_left
        if not module.startswith("hyprland/")
    ]

config["modules-left"] = modules_left

with target.open("w", encoding="utf-8") as handle:
    json.dump(config, handle, indent=4, ensure_ascii=False)
    handle.write("\n")
PY

exec waybar -c "$RUNTIME_CONFIG" -s "$STYLE" "$@"
