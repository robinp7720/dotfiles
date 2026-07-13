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

require_order() {
  local first="$1"
  local second="$2"
  local path="$3"
  local first_line
  local second_line

  first_line="$(grep -Fn -- "$first" "$ROOT/$path" | tail -n 1 | cut -d: -f1)"
  second_line="$(grep -Fn -- "$second" "$ROOT/$path" | head -n 1 | cut -d: -f1)"

  if [[ -z "$first_line" || -z "$second_line" ]]; then
    printf 'missing ordered text in %s: %s / %s\n' "$path" "$first" "$second" >&2
    exit 1
  fi

  if (( second_line <= first_line )); then
    printf 'unexpected ordering in %s: %s must appear before %s\n' "$path" "$first" "$second" >&2
    exit 1
  fi
}

make_setup_fixture() {
  local repo_root="$1"

  mkdir -p "$repo_root"
  cp "$ROOT/setup.sh" "$repo_root/setup.sh"
  touch "$repo_root/zshrc"
  mkdir -p \
    "$repo_root/scripts" \
    "$repo_root/systemd/user" \
    "$repo_root/tools/bar/target/release" \
    "$repo_root/nwg-hello" \
    "$repo_root/hypr/hyprland-config" \
    "$repo_root/hypr/monitor_layouts" \
    "$repo_root/matugen/templates" \
    "$repo_root/greetd"

  touch \
    "$repo_root/scripts/launch_kitty.sh" \
    "$repo_root/systemd/user/example.service" \
    "$repo_root/nwg-hello/hyprland.conf" \
    "$repo_root/hypr/hyprland-config/base.conf" \
    "$repo_root/hypr/monitor_layouts/default.conf" \
    "$repo_root/matugen/templates/greetd.css" \
    "$repo_root/greetd/config.toml"

  local dir
  for dir in bspwm cairo-dock dunst hypr kitty matugen eww niri nvim polybar sxhkd waybar nwg-hello greetd anyrun bar; do
    mkdir -p "$repo_root/$dir"
  done
}

make_stub_commands() {
  local stub_root="$1"

  mkdir -p "$stub_root"

  cat >"$stub_root/systemctl" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

  cat >"$stub_root/sudo" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF

  chmod +x "$stub_root/systemctl" "$stub_root/sudo"
}

verify_managed_stale_binary_link_cleanup() {
  local temp_root
  local fixture_root
  local stub_root
  local managed_home
  local foreign_home
  local stderr_log

  temp_root="$(mktemp -d)"
  trap 'rm -rf -- "$temp_root"' RETURN

  fixture_root="$temp_root/repo"
  stub_root="$temp_root/stubs"
  managed_home="$temp_root/home-managed"
  foreign_home="$temp_root/home-foreign"
  stderr_log="$temp_root/setup.stderr"

  make_setup_fixture "$fixture_root"
  make_stub_commands "$stub_root"

  mkdir -p "$managed_home/.local/bin" "$foreign_home/.local/bin"
  ln -s "$fixture_root/tools/bar/target/release/cockpit-bar" "$managed_home/.local/bin/cockpit-bar"
  ln -s "/tmp/external-cockpit-bar" "$foreign_home/.local/bin/cockpit-bar"

  PATH="$stub_root:/usr/bin:/bin" HOME="$managed_home" bash "$fixture_root/setup.sh" >/dev/null 2>"$stderr_log"
  if [[ -L "$managed_home/.local/bin/cockpit-bar" ]]; then
    printf 'managed stale cockpit-bar symlink should be removed when release binary is missing\n' >&2
    exit 1
  fi

  PATH="$stub_root:/usr/bin:/bin" HOME="$foreign_home" bash "$fixture_root/setup.sh" >/dev/null 2>"$stderr_log"
  if [[ ! -L "$foreign_home/.local/bin/cockpit-bar" ]]; then
    printf 'foreign cockpit-bar symlink should be preserved when release binary is missing\n' >&2
    exit 1
  fi
}

require_fixed 'link_path "$DIR/bar" "$HOME/.config/cockpit-bar"' "setup.sh"
require_fixed 'if [[ -x "$DIR/tools/bar/target/release/cockpit-bar" ]]; then' "setup.sh"
require_fixed 'link_path "$DIR/tools/bar/target/release/cockpit-bar" "$HOME/.local/bin/cockpit-bar"' "setup.sh"
require_fixed 'remove_managed_symlink "$HOME/.local/bin/cockpit-bar" "$DIR/tools/bar/target/release/cockpit-bar"' "setup.sh"

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
require_order 'exec-once = dbus-update-activation-environment --all' 'exec-once = systemctl --user restart cockpit-bar.service' "hypr/hyprland-config/startup.conf"
require_fixed 'layerrule = blur on, match:namespace cockpit-bar' "hypr/hyprland-config/base.conf"
require_fixed 'layerrule = ignore_alpha 0.20, match:namespace cockpit-bar' "hypr/hyprland-config/base.conf"

require_fixed '// spawn-at-startup "sh" "-lc" "~/.config/waybar/launch.sh --replace"' "niri/config.kdl"
require_fixed 'systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP NIRI_SOCKET XDG_RUNTIME_DIR XDG_SESSION_DESKTOP DBUS_SESSION_BUS_ADDRESS DISPLAY' "niri/config.kdl"
require_fixed 'systemctl --user restart cockpit-bar.service' "niri/config.kdl"
require_order 'systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP NIRI_SOCKET XDG_RUNTIME_DIR XDG_SESSION_DESKTOP DBUS_SESSION_BUS_ADDRESS DISPLAY' 'systemctl --user restart cockpit-bar.service' "niri/config.kdl"
require_fixed 'match namespace="^cockpit-bar$"' "niri/config.kdl"

require_path "waybar/launch.sh"
require_path "waybar/config"
require_fixed 'git checkout 2187ecb^ -- setup.sh hypr/hyprland-config/startup.conf hypr/hyprland-config/base.conf niri/config.kdl tools/bar/README.md' "tools/bar/README.md"
require_fixed 'rm -f systemd/user/cockpit-bar.service' "tools/bar/README.md"

verify_managed_stale_binary_link_cleanup

printf 'desktop contract ok\n'
