#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$TEMP_ROOT"' EXIT

verify() {
  local config="$1"
  if ! Hyprland --config "$config" --verify-config > "$TEMP_ROOT/verify.log" 2>&1; then
    cat "$TEMP_ROOT/verify.log" >&2
    return 1
  fi
  if ! grep -Fq 'config ok' "$TEMP_ROOT/verify.log"; then
    cat "$TEMP_ROOT/verify.log" >&2
    return 1
  fi
}

mkdir -p "$TEMP_ROOT/hypr/hyprland-config" "$TEMP_ROOT/greeter"
cp "$ROOT/hypr/hyprland.lua" "$TEMP_ROOT/hypr/"
cp "$ROOT/hypr/hyprland-config/"*.lua "$TEMP_ROOT/hypr/hyprland-config/"

# A clean install must work without a generated palette or monitor override.
HYPRLAND_PROFILE=desktop verify "$TEMP_ROOT/hypr/hyprland.lua"
HYPRLAND_PROFILE=laptop verify "$TEMP_ROOT/hypr/hyprland.lua"
verify "$TEMP_ROOT/hypr/hyprland-config/desktop.lua"
verify "$TEMP_ROOT/hypr/hyprland-config/laptop.lua"

# Render a representative palette and check its native Lua types as well.
python3 - "$ROOT" "$TEMP_ROOT/hypr/colors.lua" <<'PY'
from pathlib import Path
import re, sys
source = Path(sys.argv[1], 'matugen/templates/hyprland-colors.lua').read_text()
source = re.sub(r'\{\{colors\.\w+\.default\.hex_stripped\}\}', '83d2e3', source)
assert '{{' not in source
Path(sys.argv[2]).write_text(source)
PY
for layout in "$ROOT/hypr/monitor_layouts/"*.lua; do
  cp "$layout" "$TEMP_ROOT/hypr/monitors.lua"
  HYPRLAND_PROFILE=desktop verify "$TEMP_ROOT/hypr/hyprland.lua"
done

cat > "$TEMP_ROOT/ipc.lua" <<'LUA'
-- Constructor validation catches API/schema drift without dispatching actions.
hl.dsp.focus({ monitor = "DP-3" })
hl.dsp.focus({ workspace = "name:tv" })
hl.dsp.focus({ window = "address:0x123" })
hl.dsp.workspace.move({ workspace = "2", monitor = "DP-3" })
hl.dsp.window.move({ workspace = "2", window = "address:0x123", follow = false })
hl.dsp.window.center()
hl.dsp.window.fullscreen_state({ internal = 2, client = 2, action = "set" })
hl.dsp.dpms({ action = "off" })
hl.dsp.exit()
hl.monitor({ output = "DP-3", mode = "3840x2160@60.000", position = "-2160x1092", scale = 1.25, transform = 0, disabled = false })
LUA
verify "$TEMP_ROOT/ipc.lua"

# Greeter paths must work after installation, independently of the user's home.
cp "$ROOT/nwg-hello/hyprland.lua" "$TEMP_ROOT/greeter/"
cp "$ROOT/hypr/hyprland-config/base.lua" "$TEMP_ROOT/greeter/"
cp "$ROOT/hypr/monitor_layouts/default.lua" "$TEMP_ROOT/greeter/monitors.lua"
verify "$TEMP_ROOT/greeter/hyprland.lua"

# Exercise mode switching against a temporary HOME without moving live monitors.
mkdir -p "$TEMP_ROOT/home/.config/hypr/monitor_layouts" "$TEMP_ROOT/bin"
cp "$ROOT/hypr/monitor_layouts/"*.lua "$TEMP_ROOT/home/.config/hypr/monitor_layouts/"
cat > "$TEMP_ROOT/bin/hyprctl" <<'MOCK'
#!/usr/bin/env bash
[[ "$*" == "reload" ]] || exit 1
printf 'reload\n' >> "$RELOAD_LOG"
MOCK
chmod +x "$TEMP_ROOT/bin/hyprctl"
for mode in default game game_external; do
  HOME="$TEMP_ROOT/home" PATH="$TEMP_ROOT/bin:$PATH" RELOAD_LOG="$TEMP_ROOT/reloads" \
    bash "$ROOT/hypr/scripts/modes/$mode.sh"
  cmp "$ROOT/hypr/monitor_layouts/$mode.lua" "$TEMP_ROOT/home/.config/hypr/monitors.lua"
done
[[ "$(wc -l < "$TEMP_ROOT/reloads")" -eq 3 ]]
if HOME="$TEMP_ROOT/home" bash "$ROOT/hypr/scripts/modes/apply_layout.sh" missing.lua 2>/dev/null; then
  printf 'Missing monitor layout should fail.\n' >&2
  exit 1
fi
cmp "$ROOT/hypr/monitor_layouts/game_external.lua" "$TEMP_ROOT/home/.config/hypr/monitors.lua"

printf 'Hyprland Lua profiles, palettes, layouts, greeter, and mode switching verified.\n'
