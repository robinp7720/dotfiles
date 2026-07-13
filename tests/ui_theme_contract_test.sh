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

  actual_count="$(grep -Fc -- "$expected" "$ROOT_DIR/$file" || true)"
  if [[ "$actual_count" -ne "$expected_count" ]]; then
    printf 'Expected %s to contain %s occurrences of:\n  %s\nFound: %s\n' \
      "$file" "$expected_count" "$expected" "$actual_count" >&2
    return 1
  fi
}

assert_not_contains() {
  local file="$1"
  local rejected="$2"

  if grep -Fq -- "$rejected" "$ROOT_DIR/$file"; then
    printf 'Expected %s not to contain:\n  %s\n' "$file" "$rejected" >&2
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
assert_count waybar/style.css 2 \
  'padding: 4.5px 12px;'
assert_contains hypr/hyprland-config/base.conf \
  'layerrule = blur on, match:namespace waybar'
assert_contains hypr/hyprland-config/base.conf \
  'layerrule = ignore_alpha 0.20, match:namespace waybar'

assert_contains matugen/templates/luma.css \
  'font-family: "Cantarell", sans-serif;'
assert_contains waybar/style.css \
  'font-family: "Cantarell", "Symbols Nerd Font", sans-serif;'
assert_contains eww/eww.scss \
  'font-family: "Cantarell", sans-serif;'
assert_contains matugen/templates/dunstrc \
  'font = Cantarell 11'
assert_count hypr/hyprlock.conf 5 \
  'font_family = Cantarell'
assert_contains hypr/hyprlock.conf \
  'font_family = Cantarell ExtraBold'
assert_contains matugen/templates/greetd.css \
  'font-family: "Cantarell", sans-serif;'
assert_contains matugen/UI_STYLE.md \
  'Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback'

assert_not_contains matugen/templates/luma.css 'JetBrains Mono'
assert_not_contains waybar/style.css 'JetBrainsMono Nerd Font'
assert_not_contains eww/eww.scss 'JetBrains Mono'
assert_not_contains matugen/templates/dunstrc 'JetBrainsMono Nerd Font'
assert_not_contains hypr/hyprlock.conf 'JetBrains Mono'
assert_not_contains matugen/templates/greetd.css 'JetBrains Mono'

assert_contains bar/style.css \
  '@define-color cockpit_shell rgba(15, 20, 28, 0.84);'
assert_contains bar/style.css \
  '@define-color cockpit_panel rgba(15, 20, 28, 0.96);'
assert_contains bar/style.css \
  '@define-color cockpit_text #eef5f8;'
assert_contains bar/style.css \
  '@define-color cockpit_muted #9eacb4;'
assert_contains bar/style.css \
  'background: transparent;'
assert_contains bar/style.css \
  '.bar-island {'
assert_contains bar/style.css \
  'popover.control-center contents {'
assert_contains bar/style.css \
  '.quick-tile {'
assert_contains bar/style.css \
  'button.system-module {'
assert_not_contains bar/style.css '@surface'

printf 'Desktop UI contracts verified.\n'
