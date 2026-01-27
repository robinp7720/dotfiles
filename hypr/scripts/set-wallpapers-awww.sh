#!/usr/bin/env bash
set -euo pipefail

WALLPAPER_DIR="$HOME/Pictures/Wallpapers"
THEME_WALLPAPER="$WALLPAPER_DIR/wallhaven-21827.jpg"
WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
SOCK_OLD="$RUNTIME_DIR/${WAYLAND_DISPLAY}-awww-daemon..sock"
SOCK_NEW="$RUNTIME_DIR/${WAYLAND_DISPLAY}-awww-daemon..socket"

if ! awww query >/dev/null 2>&1; then
  rm -f "$SOCK_OLD" "$SOCK_NEW"
  awww-daemon -q >"$RUNTIME_DIR/awww-daemon.log" 2>&1 &
fi

for _ in $(seq 1 50); do
  if awww query >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! awww query >/dev/null 2>&1; then
  echo "awww daemon not ready" >&2
  exit 1
fi

awww img --outputs DVI-D-2  "$WALLPAPER_DIR/wallhaven-735pr3.jpg" --resize crop --transition-type simple --transition-step 255
awww img --outputs DP-4     "$WALLPAPER_DIR/wallhaven-202396.jpg" --resize crop --transition-type simple --transition-step 255
awww img --outputs DP-5     "$WALLPAPER_DIR/wallhaven-21827.jpg" --resize crop --transition-type simple --transition-step 255
awww img --outputs HDMI-A-2 "$WALLPAPER_DIR/wallhaven-259722.jpg" --resize crop --transition-type simple --transition-step 255
awww img --outputs HDMI-A-1 "$WALLPAPER_DIR/wallhaven-291451.jpg" --resize crop --transition-type simple --transition-step 255
awww img --outputs DP-3     "$WALLPAPER_DIR/wallhaven-2em3gy.jpg" --resize crop --transition-type simple --transition-step 255

if command -v matugen >/dev/null 2>&1; then
  matugen image "$THEME_WALLPAPER" -c "$HOME/.config/matugen/config.toml"
fi
