#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

require_fixed() {
  local text="$1"
  local path="$2"
  if ! grep -Fq -- "$text" "$path"; then
    printf 'missing text in %s: %s\n' "$path" "$text" >&2
    exit 1
  fi
}

require_path() {
  local path="$1"
  [[ -e "$path" ]] || { printf 'missing path: %s\n' "$path" >&2; exit 1; }
}

require_path "tools/tv-mode/Cargo.toml"
require_path "tools/tv-mode/src/main.rs"
require_path "tv-mode/config.toml"
require_path "systemd/user/tv-mode.service"

require_fixed 'output = "Unknown-2"' "tv-mode/config.toml"
require_fixed 'home = "3840x2160@30.0"' "tv-mode/config.toml"
require_fixed 'gaming = "1920x1080@119.88"' "tv-mode/config.toml"
require_fixed 'media = "3840x2160@30.0"' "tv-mode/config.toml"
require_fixed '[apps.kodi]' "tv-mode/config.toml"
require_fixed 'program = "/usr/bin/kodi"' "tv-mode/config.toml"
require_fixed 'card = "alsa_card.pci-0000_05_00.1"' "tv-mode/config.toml"
require_fixed 'tv_profile = "output:hdmi-stereo-extra2"' "tv-mode/config.toml"
require_fixed 'tv_sink = "alsa_output.pci-0000_05_00.1.hdmi-stereo-extra2"' "tv-mode/config.toml"
require_fixed 'ExecStopPost=%h/.local/bin/tv-mode recover' "systemd/user/tv-mode.service"
require_fixed 'bind = $mainMod, T, exec, tv-mode start' "hypr/hyprland-config/bindings.conf"
require_fixed 'match:namespace tv-mode' "hypr/hyprland-config/base.conf"
require_fixed 'link_path "$DIR/tools/tv-mode/target/release/tv-mode" "$HOME/.local/bin/tv-mode"' "setup.sh"
require_fixed 'input_path = '\''~/.config/matugen/templates/tv-mode.css'\''' "matugen/config.toml"
require_fixed 'DesktopControlOperation::TvModeStart' "tools/launcher/src/sources.rs"
require_fixed 'ControlRequest::PresentationSet' "tools/bar/src/ui/mod.rs"

printf 'tv mode desktop contract passed\n'
