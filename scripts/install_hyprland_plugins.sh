#!/usr/bin/env bash
# Build local plugins against the installed Hyprland development headers.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/hyprland/plugins"
HYMISSION="$ROOT/tools/hymission"

if [[ $# -gt 0 ]]; then
  printf 'Usage: %s\n' "$0"
  printf 'Builds and installs hymission (tools/hymission) and hypr-swipe-events.\n'
  [[ "$1" == --help || "$1" == -h ]] || exit 2
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

if [[ ! -f "$HYMISSION/CMakeLists.txt" ]]; then
  git -C "$ROOT" submodule update --init --recursive -- tools/hymission
fi
cmake -S "$HYMISSION" -B "$HYMISSION/build-cmake" -DCMAKE_BUILD_TYPE=Release
cmake --build "$HYMISSION/build-cmake" --clean-first --target hymission hymission-search-input -j 4
install_plugin "$HYMISSION/build-cmake/libhymission.so" hymission
install -Dm755 "$HYMISSION/build-cmake/hymission-search-input" "$HOME/.local/bin/hymission-search-input"

make -C "$ROOT/tools/hypr-swipe-events" install PLUGIN_DIR="$PLUGIN_DIR"
printf 'Plugins installed in %s. Restart Hyprland to load them.\n' "$PLUGIN_DIR"
