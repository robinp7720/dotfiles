#!/usr/bin/env bash
# Build local plugins against the installed Hyprland development headers.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/hyprland/plugins"

if [[ $# -gt 2 || "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf 'Usage: %s [path/to/Hyprspace-checkout [path/to/hymission-checkout]]\n' "$0"
  printf 'Installs hypr-swipe-events; optionally builds Hyprspace and hymission.\n'
  [[ $# -le 2 ]] || exit 2
  exit 0
fi

abi="$(python3 "$ROOT/scripts/hyprland_plugin_abi.py")"

install_plugin() {
  local source="$1" name="$2"
  mkdir -p "$PLUGIN_DIR"
  rm -f "$PLUGIN_DIR/$name.so.abi"
  install -m755 "$source" "$PLUGIN_DIR/$name.so"
  printf '%s\n' "$abi" > "$PLUGIN_DIR/$name.so.abi"
}

if [[ $# -ge 1 ]]; then
  source_dir="$(cd -- "$1" && pwd)"
  patch_file="$ROOT/hypr/patches/Hyprspace-gesture-coordination.patch"
  if git -C "$source_dir" apply --check "$patch_file" 2>/dev/null; then
    git -C "$source_dir" apply "$patch_file"
  elif ! git -C "$source_dir" apply --reverse --check "$patch_file" 2>/dev/null; then
    printf 'Hyprspace checkout does not match the tracked gesture patch.\n' >&2
    exit 1
  fi
  make -B -C "$source_dir" all
  install_plugin "$source_dir/Hyprspace.so" Hyprspace
fi

if [[ $# -eq 2 ]]; then
  source_dir="$(cd -- "$2" && pwd)"
  cmake -S "$source_dir" -B "$source_dir/build-cmake" -DCMAKE_BUILD_TYPE=Release
  cmake --build "$source_dir/build-cmake" --clean-first --target hymission hymission-search-input -j 4
  install_plugin "$source_dir/build-cmake/libhymission.so" hymission
  install -Dm755 "$source_dir/build-cmake/hymission-search-input" "$HOME/.local/bin/hymission-search-input"
fi

make -C "$ROOT/tools/hypr-swipe-events" install PLUGIN_DIR="$PLUGIN_DIR"
printf 'Plugins installed in %s. Restart Hyprland to load them.\n' "$PLUGIN_DIR"
