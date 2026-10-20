#!/usr/bin/env bash
# Capture a region or the focused output, save it, and copy it to the clipboard.
set -euo pipefail

mode="${1:-area}"
dir="$HOME/Pictures/Screenshots"
file="$dir/Screenshot from $(date +'%Y-%m-%d %H-%M-%S').png"

focused_output() {
  if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    hyprctl monitors -j | jq -r '.[] | select(.focused) | .name'
  elif [[ -n "${NIRI_SOCKET:-}" ]]; then
    niri msg --json focused-output | jq -r '.name'
  fi
}

mkdir -p "$dir"

case "$mode" in
  area)
    # Escape in slurp cancels without leaving an empty file behind.
    region="$(slurp -d)" || exit 0
    grim -g "$region" "$file"
    ;;
  output)
    output="$(focused_output || true)"
    if [[ -n "$output" ]]; then
      grim -o "$output" "$file"
    else
      grim "$file"
    fi
    ;;
  *)
    echo "usage: $(basename "$0") [area|output]" >&2
    exit 2
    ;;
esac

wl-copy -t image/png < "$file"
notify-send -i "$file" "Screenshot" "Saved to $file and copied to clipboard" || true
