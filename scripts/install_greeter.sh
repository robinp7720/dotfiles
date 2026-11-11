#!/usr/bin/env bash
# Narrow system installer, shared by setup and standalone greeter deployment.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
owner="${1:-${SUDO_USER:-}}"
if [[ "$(id -u)" != 0 || -z "$owner" || "$owner" == root ]]; then
  printf 'Run as root and pass the desktop username: %s USER\n' "$0" >&2
  exit 2
fi
id "$owner" >/dev/null
getent passwd greeter >/dev/null
# The clock font may be installed only in the desktop user's private font tree.
font_family="$(runuser -u "$owner" -- fc-match -f '%{family}' 0xProto)"
font_source="$(runuser -u "$owner" -- fc-match -f '%{file}' 0xProto)"
[[ "$font_family" == *0xProto* && -f "$font_source" ]] || {
  printf 'Install the 0xProto font for %s before installing the greeter.\n' "$owner" >&2
  exit 1
}
binary="$ROOT/tools/greeter/target/release/shell-greeter"
[[ -x "$binary" ]] || { printf 'Build shell-greeter in release mode first.\n' >&2; exit 1; }
"$binary" --config "$ROOT/shell-greeter/config.toml" --check-config >/dev/null

# Make validation independent of /home before switching the daemon configuration.
staging="$(mktemp -d)"
trap 'rm -rf -- "$staging"' EXIT
cp "$ROOT/shell-greeter/hyprland.lua" "$staging/hyprland.lua"
monitors="$ROOT/hypr/monitors.lua"
[[ -f "$monitors" ]] || monitors="$ROOT/hypr/monitor_layouts/default.lua"
cp "$monitors" "$staging/monitors.lua"
chmod 755 "$staging"
handoff="$ROOT/tools/greeter/target/handoff"
backend_env=()
if [[ -f "$handoff/libaquamarine.so.15" && -f "$handoff/system.sha256" ]] &&
  sha256sum --check --status "$handoff/system.sha256"; then
  install -d -m 755 "$staging/handoff"
  install -m 755 "$handoff/libaquamarine.so.15" "$staging/handoff/"
  install -m 644 "$handoff/system.sha256" "$handoff/source-revision" "$staging/handoff/"
  backend_env=("LD_LIBRARY_PATH=$staging/handoff" "AQ_PRESERVE_OUTPUTS_ON_EXIT=1")
fi
install -d -m 700 -o greeter -g "$(id -gn greeter)" "$staging/runtime"
if ! runuser -u greeter -- env XDG_RUNTIME_DIR="$staging/runtime" "${backend_env[@]}" Hyprland --config "$staging/hyprland.lua" --verify-config >"$staging/verify.log" 2>&1 ||
  ! grep -Fq 'config ok' "$staging/verify.log"; then
  cat "$staging/verify.log" >&2
  exit 1
fi

backup="/var/backups/shell-greeter/$(date +%Y%m%d-%H%M%S)-$$"
install -d -m 700 "$backup"
for target in /etc/greetd/config.toml /etc/shell-greeter /usr/local/bin/shell-greeter /usr/local/libexec/shell-greeter-session /usr/local/libexec/shell-greeter-compositor /usr/local/lib/shell-greeter/handoff /usr/local/share/fonts/shell-greeter; do
  if [[ -e "$target" || -L "$target" ]]; then
    cp -a --parents "$target" "$backup/"
  fi
done
install -d -m 755 /etc/shell-greeter /etc/greetd /usr/local/bin /usr/local/libexec
install -d -m 700 -o greeter -g "$(id -gn greeter)" /var/lib/shell-greeter
install -d -m 755 -o "$owner" -g "$(id -gn "$owner")" /var/cache/shell-greeter
if [[ ! -d /var/cache/matugen ]]; then
  install -d -m 755 -o "$owner" -g "$(id -gn "$owner")" /var/cache/matugen
fi
install -d -m 755 /usr/local/share/fonts/shell-greeter
font_target=/usr/local/share/fonts/shell-greeter/0xProto-Regular.ttf
if ! cmp -s "$font_source" "$font_target"; then
  install -m 644 "$font_source" "$font_target"
fi
fc-cache /usr/local/share/fonts/shell-greeter
# Atomic executable replacement also works if an old greeter is still running.
install -m 755 "$binary" /usr/local/bin/.shell-greeter-new
mv -f /usr/local/bin/.shell-greeter-new /usr/local/bin/shell-greeter
install -m 755 "$ROOT/shell-greeter/session.sh" /usr/local/libexec/shell-greeter-session
install -m 755 "$ROOT/shell-greeter/compositor.sh" /usr/local/libexec/shell-greeter-compositor
handoff="$staging/handoff"
if [[ -d "$handoff" ]]; then
  destination=/usr/local/lib/shell-greeter/handoff
  install -d -m 755 "$destination"
  install -m 755 "$handoff/libaquamarine.so.15" "$destination/.library-new"
  mv -f "$destination/.library-new" "$destination/libaquamarine.so.15"
  install -m 644 "$handoff/system.sha256" "$destination/.manifest-new"
  mv -f "$destination/.manifest-new" "$destination/system.sha256"
  install -m 644 "$handoff/source-revision" "$destination/source-revision"
else
  printf 'No current private build supplied; retaining any installed handoff backend.\n' >&2
fi
install -m 644 "$ROOT/shell-greeter/config.toml" /etc/shell-greeter/config.toml
install -m 644 "$staging/hyprland.lua" /etc/shell-greeter/hyprland.lua
install -m 644 "$staging/monitors.lua" /etc/shell-greeter/monitors.lua
# Switch greetd only once every referenced executable and config is installed.
install -m 644 "$ROOT/greetd/config.toml" /etc/greetd/.shell-greeter-new
mv -f /etc/greetd/.shell-greeter-new /etc/greetd/config.toml
printf 'Installed shell-greeter. Previous configuration: %s\n' "$backup"
printf 'The current session was not restarted. The new daemon configuration takes effect after greetd restarts (or on reboot).\n'
