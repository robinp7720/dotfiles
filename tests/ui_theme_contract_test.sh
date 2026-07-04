#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

assert_contains() {
  local file="$1"
  local expected="$2"

  if ! grep -Fq -- "$expected" "$ROOT_DIR/$file"; then
    printf 'Expected %s to contain:\n  %s\n' "$file" "$expected" >&2
    return 1
  fi
}

assert_count() {
  local file="$1"
  local expected_count="$2"
  local expected="$3"
  local actual_count

  actual_count="$(grep -Fc -- "$expected" "$ROOT_DIR/$file")"
  if [[ "$actual_count" -ne "$expected_count" ]]; then
    printf 'Expected %s to contain %s occurrences of:\n  %s\nFound: %s\n' \
      "$file" "$expected_count" "$expected" "$actual_count" >&2
    return 1
  fi
}

assert_contains matugen/templates/luma.css \
  'background-image: linear-gradient(180deg, alpha(@luma_surface_high, 0.72), alpha(@luma_surface, 0.68));'
assert_contains matugen/templates/luma.css \
  'background-color: alpha(@luma_surface_highest, 0.66);'
assert_count matugen/templates/luma.css 3 \
  'background-color: transparent;'
assert_contains waybar/style.css \
  '@define-color bar_bg alpha(@surface_container_lowest, 0.58);'
assert_contains waybar/style.css \
  '@define-color module_bg alpha(@surface_container, 0.52);'
assert_contains waybar/style.css \
  '@define-color module_bg_alt alpha(@surface_container_high, 0.60);'
assert_contains hypr/hyprland-config/base.conf \
  'layerrule = blur on, match:namespace waybar'
assert_contains hypr/hyprland-config/base.conf \
  'layerrule = ignore_alpha 0.20, match:namespace waybar'

printf 'Balanced Glass UI contract verified.\n'
