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
  'background-image: linear-gradient(180deg, alpha(@luma_surface_high, 0.94), alpha(@luma_surface, 0.96));'
assert_contains matugen/templates/luma.css \
  'background-color: alpha(@luma_surface_highest, 0.66);'
assert_count matugen/templates/luma.css 3 \
  'background-color: transparent;'
assert_contains hypr/hyprlock.conf \
  'source = ~/.config/hypr/hyprlock-colors.conf'
assert_contains matugen/config.toml \
  "input_path = '~/.config/matugen/templates/hyprlock-colors.conf'"
assert_contains matugen/templates/hyprlock-colors.conf \
  '$lock_error = rgb({{colors.error.default.hex_stripped}})'

assert_contains matugen/templates/luma.css \
  'font-family: "Cantarell", sans-serif;'
assert_count hypr/hyprlock.conf 11 \
  'font_family = Cantarell'
assert_contains hypr/hyprlock.conf \
  'font_family = Cantarell ExtraBold'
assert_count hypr/hyprlock.conf 11 \
  'monitor = # @dashboard-monitor@'
assert_not_contains hypr/hyprlock.conf \
  'shape {'
assert_contains hypr/hyprlock.conf \
  'text = cmd[update:1000] ~/.dotfiles/scripts/lockscreen_context.sh timer'
assert_contains hypr/hyprlock.conf \
  'text = cmd[update:5000] ~/.dotfiles/scripts/lockscreen_context.sh summary'
assert_contains hypr/hyprlock.conf \
  'onclick = ~/.dotfiles/scripts/now_playing.sh --toggle'
assert_contains matugen/templates/greetd.css \
  'font-family: "Cantarell", sans-serif;'
assert_contains matugen/UI_STYLE.md \
  'Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback'

assert_not_contains matugen/templates/luma.css 'JetBrains Mono'
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
  '.context-island {'
assert_not_contains bar/style.css \
  'min-width: 360px;'
assert_not_contains bar/style.css \
  '.context-chip.is-playing .context-icon,'
assert_not_contains bar/style.css \
  'button.system-module.charging image,'
assert_contains bar/style.css \
  'popover.control-center contents {'
assert_contains bar/style.css \
  '.quick-tile {'
assert_contains bar/style.css \
  'button.system-module {'
assert_not_contains bar/style.css '@surface'

# Warm launcher and TV sessions follow wallpaper-driven theme changes.
assert_contains tools/launcher/src/ui/theme.rs 'monitor_file('
assert_contains tools/tv-mode/src/ui.rs 'self.state["theme"] = theme();'

printf 'Desktop UI contracts verified.\n'
