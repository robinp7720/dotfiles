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

reject_fixed() {
  local needle="$1"
  local path="$2"

  if grep -Fq -- "$needle" "$ROOT/$path"; then
    printf 'unexpected text in %s: %s\n' "$path" "$needle" >&2
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
    "$repo_root/tools/launcher/target/release" \
    "$repo_root/nwg-hello" \
    "$repo_root/hypr/hyprland-config" \
    "$repo_root/hypr/monitor_layouts" \
    "$repo_root/matugen/templates" \
    "$repo_root/greetd"

  touch \
    "$repo_root/scripts/launch_kitty.sh" \
    "$repo_root/systemd/user/example.service" \
    "$repo_root/tools/launcher/target/release/Luma" \
    "$repo_root/nwg-hello/hyprland.conf" \
    "$repo_root/hypr/hyprland-config/base.conf" \
    "$repo_root/hypr/monitor_layouts/default.conf" \
    "$repo_root/matugen/templates/greetd.css" \
    "$repo_root/greetd/config.toml"

  chmod +x "$repo_root/tools/launcher/target/release/Luma"

  local dir
  for dir in bspwm cairo-dock dunst hypr kitty matugen eww niri nvim polybar sxhkd waybar nwg-hello greetd anyrun bar; do
    mkdir -p "$repo_root/$dir"
  done
}

verify_luma_binary_link() {
  local temp_root
  local fixture_root
  local stub_root
  local test_home

  temp_root="$(mktemp -d)"
  trap 'rm -rf -- "$temp_root"' RETURN

  fixture_root="$temp_root/repo"
  stub_root="$temp_root/stubs"
  test_home="$temp_root/home"

  make_setup_fixture "$fixture_root"
  make_stub_commands "$stub_root"
  mkdir -p "$test_home"

  PATH="$stub_root:/usr/bin:/bin" HOME="$test_home" bash "$fixture_root/setup.sh" >/dev/null 2>&1

  if [[ ! -L "$test_home/.local/bin/Luma" ]] \
    || [[ "$(readlink "$test_home/.local/bin/Luma")" != "$fixture_root/tools/launcher/target/release/Luma" ]]; then
    printf 'setup should expose the built Luma binary in ~/.local/bin\n' >&2
    exit 1
  fi
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

require_fixed 'let root = gtk::CenterBox::new();' "tools/bar/src/ui/surface.rs"
require_fixed 'center_slot.set_size_request(CENTER_SLOT_MIN_WIDTH, -1);' "tools/bar/src/ui/surface.rs"
require_fixed 'context_stack.set_hhomogeneous(false);' "tools/bar/src/ui/surface.rs"
require_fixed 'context_label.set_xalign(0.5);' "tools/bar/src/ui/surface.rs"
require_fixed 'root.set_center_widget(Some(&center_slot));' "tools/bar/src/ui/surface.rs"
require_fixed 'system_box.append(&system_items);' "tools/bar/src/ui/surface.rs"
require_fixed 'status_button.update(&module.button);' "tools/bar/src/ui/surface.rs"
require_fixed 'center_for_click.show_page(focus);' "tools/bar/src/ui/surface.rs"
require_fixed 'window.set_default_size(512, 680);' "tools/bar/src/ui/control_center.rs"
require_fixed 'root.set_size_request(480, 648);' "tools/bar/src/ui/control_center.rs"
require_fixed 'window.set_namespace(Some("cockpit-control-center"));' "tools/bar/src/ui/control_center.rs"
require_fixed 'window.set_layer(Layer::Overlay);' "tools/bar/src/ui/control_center.rs"
require_fixed 'window.set_exclusive_zone(0);' "tools/bar/src/ui/control_center.rs"
require_fixed 'stack.set_hhomogeneous(true);' "tools/bar/src/ui/control_center.rs"
require_fixed 'Self::Bluetooth => "bluetooth"' "tools/bar/src/ui/control_center.rs"
require_fixed 'nav_for_back.navigate(ControlCenterFocus::Overview, true);' "tools/bar/src/ui/control_center.rs"
require_fixed 'Open {} in Luma' "tools/bar/src/ui/control_center.rs"
require_fixed 'fn reconcile_timers(' "tools/bar/src/ui/control_center.rs"
require_fixed 'update_timer_row(widgets.get(&timer.id).expect("timer row"), timer);' "tools/bar/src/ui/control_center.rs"
require_fixed 'previous.add_css_class("media-transport-secondary");' "tools/bar/src/ui/control_center.rs"
require_fixed 'play.add_css_class("media-transport-primary");' "tools/bar/src/ui/control_center.rs"
require_fixed 'root.set_size_request(-1, 140);' "tools/bar/src/ui/control_center.rs"
require_fixed 'artwork.set_content_fit(gtk::ContentFit::Cover);' "tools/bar/src/ui/control_center.rs"
require_fixed 'self.artwork.show(' "tools/bar/src/ui/control_center.rs"
require_fixed '{{mpris:artUrl}}' "tools/bar/src/sources/media.rs"
require_fixed '&& same_track(previous, &event)' "tools/bar/src/sources/media.rs"
require_fixed 'const PREFERRED_PLAYER_FAMILIES: &[&str] = &[' "tools/bar/src/sources/media.rs"
require_fixed 'const BROWSER_PLAYER_FAMILIES: &[&str] = &[' "tools/bar/src/sources/media.rs"
require_fixed '.min_by_key(|state| player_selection_key(state))' "tools/bar/src/sources/media.rs"
require_fixed 'pub player: String,' "tools/bar/src/ui/control_center.rs"
require_fixed 'pub fn media_player(&self) -> Option<String> {' "tools/bar/src/ui/control_center.rs"
require_fixed 'ActionIntent::ControlMedia {' "tools/bar/src/ui/control_center.rs"
require_fixed 'format!("--player={player}")' "tools/bar/src/actions.rs"
require_fixed 'Duration::from_millis(300)' "tools/bar/src/ui/control_center.rs"
require_fixed 'if !track_changed && uri.is_none() {' "tools/bar/src/ui/control_center.rs"
require_fixed 'prefer_artwork_candidate(' "tools/bar/src/ui/control_center.rs"
require_fixed 'pub generation: u64,' "tools/bar/src/ui/artwork.rs"
require_fixed 'pub art_url: Option<String>,' "tools/bar/src/model.rs"
require_fixed 'const MAX_ARTWORK_BYTES: u64 = 8 * 1024 * 1024;' "tools/bar/src/ui/artwork.rs"
require_fixed 'const MAX_CACHE_FILES: usize = 64;' "tools/bar/src/ui/artwork.rs"
require_fixed '.media-card .media-transport-primary {' "bar/style.css"
require_fixed 'min-height: 140px;' "bar/style.css"
require_fixed '.control-slider-row scale:hover slider,' "bar/style.css"
require_fixed '.control-slider-icon {' "bar/style.css"
require_fixed '.control-slider-row button.volume-mute-button {' "bar/style.css"
require_fixed '.control-slider-row button.volume-mute-button.muted {' "bar/style.css"
require_fixed '.slider-value {' "bar/style.css"
require_fixed '.metric-segment.active {' "bar/style.css"
require_fixed '.metric-card.warning .metric-segment.active {' "bar/style.css"
require_fixed '.metric-card.critical .metric-segment.active {' "bar/style.css"
require_fixed 'const METRIC_SEGMENTS: usize = 10;' "tools/bar/src/ui/control_center.rs"
require_fixed 'fn metric_visual(percent: Option<u8>) -> MetricVisual {' "tools/bar/src/ui/control_center.rs"
require_fixed 'pub wifi_available: bool,' "tools/bar/src/model.rs"
require_fixed 'pub ethernet_available: bool,' "tools/bar/src/model.rs"
require_fixed 'pub battery_present: bool,' "tools/bar/src/model.rs"
require_fixed '.call("GetDevices", &())' "tools/bar/src/sources/network.rs"
require_fixed 'tile.toggle.set_visible(spec.toggle_available);' "tools/bar/src/ui/control_center.rs"
require_fixed 'fn reflow_quick_grid' "tools/bar/src/ui/control_center.rs"
require_fixed 'fn volume_slider_row()' "tools/bar/src/ui/control_center.rs"
require_fixed 'fn volume_icon_name(muted: bool, percent: Option<u8>)' "tools/bar/src/ui/control_center.rs"
require_fixed 'handle.send("toggle-mute", ActionIntent::ToggleMute);' "tools/bar/src/ui/control_center.rs"
reject_fixed 'Sound output' "tools/bar/src/ui/control_center.rs"
reject_fixed 'audio_switch' "tools/bar/src/ui/control_center.rs"
require_fixed '.set_visible(spec.battery_present);' "tools/bar/src/ui/control_center.rs"
require_fixed 'power_profile_icon(&snapshot.system.power.profile)' "tools/bar/src/ui/system.rs"
require_fixed 'window.control-center-window button:focus-visible,' "bar/style.css"
require_fixed '@keyframes control-center-enter {' "bar/style.css"
require_fixed '@keyframes control-center-exit {' "bar/style.css"
require_fixed '.control-center-root.control-center-entering {' "bar/style.css"
require_fixed '.control-center-root.control-center-exiting {' "bar/style.css"
require_fixed '@media (prefers-reduced-motion: reduce) {' "bar/style.css"
reject_fixed 'border: 2px solid @primary;' "bar/style.css"
require_fixed 'center_for_click.current_page() == focus' "tools/bar/src/ui/surface.rs"
require_fixed 'if !popover.is_visible() {' "tools/bar/src/ui/surface.rs"
require_fixed 'fn show_managed_window(' "tools/bar/src/ui/surface.rs"
require_fixed 'ManagedOverlay::ControlCenter(control_center.clone())' "tools/bar/src/ui/surface.rs"
require_fixed 'control_center.present();' "tools/bar/src/ui/surface.rs"
require_fixed 'center_for_click.dismiss();' "tools/bar/src/ui/surface.rs"
require_fixed 'pub fn dismiss(&self) {' "tools/bar/src/ui/control_center.rs"
require_fixed 'ControlCenterMotionEvent::Present' "tools/bar/src/ui/control_center.rs"
require_fixed 'ControlCenterMotionEvent::Dismiss' "tools/bar/src/ui/control_center.rs"
reject_fixed 'surface.dismiss_popovers();' "tools/bar/src/ui/surface.rs"
require_fixed 'window.set_margin(Edge::Top, top_margin);' "tools/bar/src/ui/control_center.rs"
reject_fixed 'BAR_HEIGHT + SURFACE_MARGIN * 2,' "tools/bar/src/ui/surface.rs"
require_fixed 'layerrule = no_anim on, match:namespace cockpit-control-center' "hypr/hyprland-config/base.conf"

require_path "waybar/launch.sh"
require_path "waybar/config"
require_fixed 'git checkout 2187ecb^ -- setup.sh hypr/hyprland-config/startup.conf hypr/hyprland-config/base.conf niri/config.kdl tools/bar/README.md' "tools/bar/README.md"
require_fixed 'rm -f systemd/user/cockpit-bar.service' "tools/bar/README.md"

verify_managed_stale_binary_link_cleanup
verify_luma_binary_link

printf 'desktop contract ok\n'
