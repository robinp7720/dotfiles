#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HYPR="hypr/hyprland-config/bindings.lua"
NIRI="niri/config.kdl"

assert_contains() {
  local file="$1"
  local expected="$2"

  if ! grep -Fq -- "$expected" "$ROOT_DIR/$file"; then
    printf 'Expected %s to contain:\n  %s\n' "$file" "$expected" >&2
    return 1
  fi
}

# Each shared surface keeps the same shortcut and toggle semantics on both
# compositors: "hyprland key|hyprland command|niri key|niri spawn args".
shared_bindings=(
  'SUPER + SPACE|Luma --toggle|Mod+Space|"Luma" "--toggle"'
  'SUPER + TAB|Luma --mode windows --toggle|Mod+Tab|"Luma" "--mode" "windows" "--toggle"'
  'SUPER + SHIFT + SPACE|Luma --mode controls --toggle|Mod+Shift+Space|"Luma" "--mode" "controls" "--toggle"'
  'SUPER + P|Luma --mode pass --toggle|Mod+P|"Luma" "--mode" "pass" "--toggle"'
  'SUPER + B|vigil control-center toggle"|Mod+B|"vigil" "control-center" "toggle";'
  'SUPER + SHIFT + N|vigil control-center toggle --context notifications|Mod+Shift+N|"vigil" "control-center" "toggle" "--context" "notifications"'
  'SUPER + Print|~/.dotfiles/scripts/screenshot.sh area|Super+Print|"~/.dotfiles/scripts/screenshot.sh" "area"'
  'SUPER + SHIFT + V|Luma --mode clipboard --toggle|Mod+Shift+V|"Luma" "--mode" "clipboard" "--toggle"'
)

for entry in "${shared_bindings[@]}"; do
  IFS='|' read -r hypr_key hypr_cmd niri_key niri_cmd <<< "$entry"
  assert_contains "$HYPR" "hl.bind(\"$hypr_key\", hl.dsp.exec_cmd(\"${hypr_cmd%\"}"
  if ! grep -E "^\s*${niri_key//+/\\+} " "$ROOT_DIR/$NIRI" | grep -Fq -- "$niri_cmd"; then
    printf 'Expected %s to bind %s to:\n  %s\n' "$NIRI" "$niri_key" "$niri_cmd" >&2
    exit 1
  fi
done

assert_contains "$HYPR" 'wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.05+ -l 1.0 && vigil osd volume'
assert_contains "$NIRI" 'wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.05+ -l 1.0 && vigil osd volume'
assert_contains "$HYPR" 'set-mute @DEFAULT_AUDIO_SOURCE@ toggle && vigil osd microphone'
assert_contains "$NIRI" 'set-mute @DEFAULT_AUDIO_SOURCE@ toggle && vigil osd microphone'

printf 'compositor binding parity: ok\n'
