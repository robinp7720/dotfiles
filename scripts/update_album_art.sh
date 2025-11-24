#!/usr/bin/env bash
# Fetch current album art for Hyprlock and cache it locally.

set -euo pipefail

CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/hyprlock"
OUT="$CACHE_DIR/album.jpg"
FALLBACK="${HOME}/Pictures/Wallpapers/PXL_20230314_080809539.jpg"

mkdir -p "$CACHE_DIR"

art_url="$(playerctl metadata mpris:artUrl 2>/dev/null || true)"
local_path=""

if [[ -n "$art_url" ]]; then
  case "$art_url" in
    file://*)
      local_path="${art_url#file://}"
      ;;
    http://*|https://*)
      tmp="$(mktemp "$CACHE_DIR/artXXXX")"
      if curl -fsSL --max-time 5 "$art_url" -o "$tmp"; then
        mv "$tmp" "$OUT"
        exit 0
      fi
      ;;
    *)
      if [ -f "$art_url" ]; then
        local_path="$art_url"
      fi
      ;;
  esac
fi

if [ -n "$local_path" ] && [ -f "$local_path" ]; then
  cp "$local_path" "$OUT"
  exit 0
fi

# Fallback image so Hyprlock always has something to show
if [ -f "$FALLBACK" ]; then
  cp "$FALLBACK" "$OUT"
else
  : > "$OUT"  # create empty file to keep hyprlock happy
fi
