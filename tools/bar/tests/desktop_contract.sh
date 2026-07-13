#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

require_fixed() {
  local needle="$1"
  local path="$2"

  if ! grep -Fq -- "$needle" "$ROOT/$path"; then
    printf 'missing required text in %s: %s\n' "$path" "$needle" >&2
    exit 1
  fi
}

require_path() {
  local path="$1"

  if [[ ! -e "$ROOT/$path" ]]; then
    printf 'missing required path: %s\n' "$path" >&2
    exit 1
  fi
}

require_fixed 'link_path "$DIR/bar" "$HOME/.config/cockpit-bar"' "setup.sh"
require_fixed 'if [[ -x "$DIR/tools/bar/target/release/cockpit-bar" ]]; then' "setup.sh"
require_fixed 'link_path "$DIR/tools/bar/target/release/cockpit-bar" "$HOME/.local/bin/cockpit-bar"' "setup.sh"

require_path "systemd/user/cockpit-bar.service"
require_fixed 'Type=simple' "systemd/user/cockpit-bar.service"
require_fixed 'ExecStart=%h/.local/bin/cockpit-bar' "systemd/user/cockpit-bar.service"
require_fixed 'Restart=on-failure' "systemd/user/cockpit-bar.service"
require_fixed 'RestartSec=2' "systemd/user/cockpit-bar.service"
require_fixed 'After=graphical-session.target' "systemd/user/cockpit-bar.service"
require_fixed 'PartOf=graphical-session.target' "systemd/user/cockpit-bar.service"
require_fixed 'WantedBy=graphical-session.target' "systemd/user/cockpit-bar.service"

require_fixed '# exec = ~/.config/waybar/launch.sh --replace &' "hypr/hyprland-config/startup.conf"
require_fixed 'exec-once = systemctl --user restart cockpit-bar.service' "hypr/hyprland-config/startup.conf"
require_fixed 'layerrule = blur on, match:namespace cockpit-bar' "hypr/hyprland-config/base.conf"
require_fixed 'layerrule = ignore_alpha 0.20, match:namespace cockpit-bar' "hypr/hyprland-config/base.conf"

require_fixed '// spawn-at-startup "sh" "-lc" "~/.config/waybar/launch.sh --replace"' "niri/config.kdl"
require_fixed 'systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP NIRI_SOCKET XDG_RUNTIME_DIR XDG_SESSION_DESKTOP DBUS_SESSION_BUS_ADDRESS DISPLAY' "niri/config.kdl"
require_fixed 'systemctl --user restart cockpit-bar.service' "niri/config.kdl"
require_fixed 'match namespace="^cockpit-bar$"' "niri/config.kdl"

require_path "waybar/launch.sh"
require_path "waybar/config"

printf 'desktop contract ok\n'
