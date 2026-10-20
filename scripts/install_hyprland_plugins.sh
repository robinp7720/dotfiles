#!/usr/bin/env bash
# Build local plugins against the installed Hyprland development headers.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/hyprland/plugins"

if [[ $# -gt 1 || "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf 'Usage: %s [path/to/Hyprspace-checkout]\n' "$0"
  printf 'Installs hypr-swipe-events; optionally patches and installs Hyprspace.\n'
  [[ $# -le 1 ]] || exit 2
  exit 0
fi

if [[ $# -eq 1 ]]; then
  source_dir="$(cd -- "$1" && pwd)"
  patch_file="$ROOT/hypr/patches/Hyprspace-gesture-coordination.patch"
  if git -C "$source_dir" apply --check "$patch_file" 2>/dev/null; then
    git -C "$source_dir" apply "$patch_file"
  elif ! git -C "$source_dir" apply --reverse --check "$patch_file" 2>/dev/null; then
    printf 'Hyprspace checkout does not match the tracked gesture patch.\n' >&2
    exit 1
  fi
  make -C "$source_dir" all
  install -Dm755 "$source_dir/Hyprspace.so" "$PLUGIN_DIR/Hyprspace.so"
fi

make -C "$ROOT/tools/hypr-swipe-events" install PLUGIN_DIR="$PLUGIN_DIR"
printf 'Plugins installed in %s. Restart Hyprland to load them.\n' "$PLUGIN_DIR"
