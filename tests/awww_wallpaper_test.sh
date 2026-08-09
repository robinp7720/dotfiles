#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$TEMP_ROOT"' EXIT

mkdir -p "$TEMP_ROOT/bin" "$TEMP_ROOT/runtime"
touch "$TEMP_ROOT/wallpaper.png"
printf 'default=%s\n' "$TEMP_ROOT/wallpaper.png" >"$TEMP_ROOT/wallpapers.conf"

cat >"$TEMP_ROOT/bin/awww" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  query)
    count=0
    [[ ! -f "$TEST_ROOT/query-count" ]] || count="$(<"$TEST_ROOT/query-count")"
    count=$((count + 1))
    printf '%s\n' "$count" >"$TEST_ROOT/query-count"
    (( count >= 3 ))
    ;;
  img)
    printf '%s\n' "$*" >>"$TEST_ROOT/awww.log"
    ;;
  *)
    exit 2
    ;;
esac
EOF

cat >"$TEMP_ROOT/bin/hyprctl" <<'EOF'
#!/usr/bin/env bash
[[ "${1:-}" == "monitors" ]] || exit 2
printf '%s\n' 'Monitor DP-1 (ID 0):'
EOF
cat >"$TEMP_ROOT/bin/matugen" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >"$TEST_ROOT/matugen.log"
EOF
chmod +x "$TEMP_ROOT/bin/awww" "$TEMP_ROOT/bin/hyprctl" "$TEMP_ROOT/bin/matugen"

TEST_ROOT="$TEMP_ROOT" \
PATH="$TEMP_ROOT/bin:/usr/bin:/bin" \
XDG_RUNTIME_DIR="$TEMP_ROOT/runtime" \
WAYLAND_DISPLAY="wayland-test" \
AWWW_WALLPAPER_CONFIG="$TEMP_ROOT/wallpapers.conf" \
  "$ROOT/scripts/awww_wallpaper_watcher.sh" --once --no-theme

[[ "$(<"$TEMP_ROOT/query-count")" -ge 3 ]] \
  || { printf 'watcher did not wait for the supervised daemon\n' >&2; exit 1; }
grep -Fq -- 'img --outputs DP-1' "$TEMP_ROOT/awww.log" \
  || { printf 'watcher did not apply the configured wallpaper\n' >&2; exit 1; }

TEST_ROOT="$TEMP_ROOT" \
PATH="$TEMP_ROOT/bin:/usr/bin:/bin" \
XDG_RUNTIME_DIR="$TEMP_ROOT/runtime" \
WAYLAND_DISPLAY="wayland-test" \
AWWW_WALLPAPER_CONFIG="$TEMP_ROOT/wallpapers.conf" \
HOME="$TEMP_ROOT" \
  "$ROOT/scripts/awww_wallpaper_watcher.sh" --once

grep -Fq -- "image $TEMP_ROOT/wallpaper.png --prefer saturation" "$TEMP_ROOT/matugen.log" \
  || { printf 'watcher did not make Matugen source selection deterministic\n' >&2; exit 1; }

printf 'ok - awww wallpaper watcher contract passed\n'
