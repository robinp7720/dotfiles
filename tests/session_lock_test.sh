#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

assert_contains() {
  local file="$1"
  local expected="$2"
  grep -Fq -- "$expected" "$file" || fail "$file did not contain: $expected"
}

make_stubs() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"

  cat > "$bin_dir/hyprctl" <<'EOF'
#!/usr/bin/env bash
if [[ "${TEST_COMPOSITOR:-hyprland}" != "hyprland" ]]; then exit 1; fi
printf '%s\n' "$*" >> "${TEST_LOG:?}"
case "$*" in
  "monitors -j") printf '%s\n' '[]' ;;
  "monitors") printf '%s\n' 'Monitor DP-5' ;;
  "activeworkspace -j") printf '%s\n' '{"name":"2","monitor":"DP-5"}' ;;
esac
EOF

  cat > "$bin_dir/niri" <<'EOF'
#!/usr/bin/env bash
if [[ "${TEST_COMPOSITOR:-hyprland}" != "niri" ]]; then exit 1; fi
printf '%s\n' "$*" >> "${TEST_LOG:?}"
case "$*" in
  "msg outputs") printf '%s\n' 'Output DP-4' ;;
  "msg --json focused-output") printf '%s\n' '{"name":"DP-4"}' ;;
esac
EOF

  cat > "$bin_dir/pgrep" <<'EOF'
#!/usr/bin/env bash
exit "${TEST_PGREP_EXIT:-1}"
EOF

  cat > "$bin_dir/hyprlock" <<'EOF'
#!/usr/bin/env bash
printf 'hyprlock %s\n' "$*" >> "${TEST_LOG:?}"
exit "${TEST_HYPRLOCK_EXIT:-0}"
EOF

  cat > "$bin_dir/swaylock" <<'EOF'
#!/usr/bin/env bash
printf 'swaylock %s\n' "$*" >> "${TEST_LOG:?}"
EOF

  cat > "$bin_dir/vigil" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" == "lockscreen snapshot" ]]; then
  printf '%s\n' '{"type":"lockscreen","snapshot":{"active_timer_label":"Tea & code","active_timer_remaining_seconds":90,"network_label":"Ethernet & lab","power_label":"Performance","unread_notifications":2}}'
else
  printf 'vigil %s\n' "$*" >> "${TEST_LOG:?}"
fi
EOF

  cat > "$bin_dir/noop" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

  cat > "$bin_dir/render" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${TEST_RUNTIME_CONFIG:?}"
EOF

  cat > "$bin_dir/brightnessctl" <<'EOF'
#!/usr/bin/env bash
printf 'brightnessctl %s\n' "$*" >> "${TEST_LOG:?}"
EOF

  chmod +x "$bin_dir"/*
}

test_runtime_renderer() {
  local workdir="$1"
  local bin_dir="$2"
  local rendered="$workdir/rendered.conf"

  PATH="$bin_dir:/usr/bin:/bin" \
  TEST_LOG="$workdir/renderer.log" \
    "$ROOT_DIR/scripts/render_hyprlock_config.sh" \
      --output DP-5 \
      --destination "$rendered" >/dev/null
  assert_contains "$rendered" 'monitor = DP-5'
  [[ "$(grep -Fc 'monitor = DP-5' "$rendered")" -eq 11 ]] ||
    fail "renderer did not target every dashboard widget"
  [[ "$(stat -c %a "$rendered")" == "600" ]] || fail "runtime config is not user-only"

  PATH="$bin_dir:/usr/bin:/bin" \
  TEST_LOG="$workdir/renderer.log" \
    "$ROOT_DIR/scripts/render_hyprlock_config.sh" \
      --output 'DP-5;exec=unsafe' \
      --destination "$rendered" >/dev/null
  ! grep -Fq 'exec=unsafe' "$rendered" || fail "unsafe connector reached generated config"

  : > "$workdir/renderer.log"
  PATH="$bin_dir:/usr/bin:/bin" \
  TEST_LOG="$workdir/renderer.log" \
  TEST_COMPOSITOR=hyprland \
    "$ROOT_DIR/scripts/render_hyprlock_config.sh" \
      --destination "$rendered" >/dev/null
  assert_contains "$rendered" 'monitor = DP-5'

  : > "$workdir/renderer.log"
  PATH="$bin_dir:/usr/bin:/bin" \
  TEST_LOG="$workdir/renderer.log" \
  TEST_COMPOSITOR=niri \
    "$ROOT_DIR/scripts/render_hyprlock_config.sh" \
      --destination "$rendered" >/dev/null
  assert_contains "$rendered" 'monitor = DP-4'
}

test_shared_lock_launcher() {
  local workdir="$1"
  local bin_dir="$2"
  local log="$workdir/lock.log"
  local runtime_config="$workdir/runtime.conf"
  : > "$runtime_config"
  : > "$log"

  PATH="$bin_dir:/usr/bin:/bin" \
  HOME="$workdir/home" \
  TEST_LOG="$log" \
  TEST_RUNTIME_CONFIG="$runtime_config" \
  HYPRLOCK_BIN="$bin_dir/hyprlock" \
  SWAYLOCK_BIN="$bin_dir/swaylock" \
  VIGIL_BIN="$bin_dir/vigil" \
  RENDER_CONFIG_BIN="$bin_dir/render" \
  ALBUM_ART_UPDATE_BIN="$bin_dir/noop" \
    "$ROOT_DIR/scripts/session_lock.sh"

  assert_contains "$log" 'vigil control-center close'
  assert_contains "$log" "hyprlock --config $runtime_config --immediate-render --no-fade-in"

  : > "$log"
  PATH="$bin_dir:/usr/bin:/bin" \
  HOME="$workdir/home" \
  TEST_LOG="$log" \
  TEST_RUNTIME_CONFIG="$runtime_config" \
  TEST_HYPRLOCK_EXIT=19 \
  HYPRLOCK_BIN="$bin_dir/hyprlock" \
  SWAYLOCK_BIN="$bin_dir/swaylock" \
  VIGIL_BIN="" \
  RENDER_CONFIG_BIN="$bin_dir/render" \
  ALBUM_ART_UPDATE_BIN="$bin_dir/noop" \
    "$ROOT_DIR/scripts/session_lock.sh"
  assert_contains "$log" 'swaylock --daemonize'

  : > "$log"
  PATH="$bin_dir:/usr/bin:/bin" \
  HOME="$workdir/home" \
  TEST_LOG="$log" \
  TEST_RUNTIME_CONFIG="$runtime_config" \
  TEST_PGREP_EXIT=0 \
  HYPRLOCK_BIN="$bin_dir/hyprlock" \
  SWAYLOCK_BIN="$bin_dir/swaylock" \
  VIGIL_BIN="$bin_dir/vigil" \
  RENDER_CONFIG_BIN="$bin_dir/render" \
  ALBUM_ART_UPDATE_BIN="$bin_dir/noop" \
    "$ROOT_DIR/scripts/session_lock.sh"
  [[ ! -s "$log" ]] || fail "active locker should make the launcher a no-op"
}

test_compositor_dpms_and_context() {
  local workdir="$1"
  local bin_dir="$2"
  local log="$workdir/actions.log"
  : > "$log"

  PATH="$bin_dir:/usr/bin:/bin" TEST_LOG="$log" TEST_COMPOSITOR=hyprland \
    "$ROOT_DIR/scripts/session_display.sh" off
  assert_contains "$log" 'dispatch hl.dsp.dpms({ action = "off" })'

  : > "$log"
  PATH="$bin_dir:/usr/bin:/bin" TEST_LOG="$log" TEST_COMPOSITOR=niri \
    "$ROOT_DIR/scripts/session_display.sh" on
  assert_contains "$log" 'msg action power-on-monitors'

  summary="$(VIGIL_BIN="$bin_dir/vigil" "$ROOT_DIR/scripts/lockscreen_context.sh" summary)"
  timer="$(VIGIL_BIN="$bin_dir/vigil" "$ROOT_DIR/scripts/lockscreen_context.sh" timer)"
  [[ "$summary" == 'Ethernet &amp; lab  ·  Performance  ·  2 unread' ]] ||
    fail "unexpected lockscreen summary: $summary"
  [[ "$timer" == 'Focus · Tea &amp; code · 1:30' ]] ||
    fail "unexpected lockscreen timer: $timer"
}

main() {
  local workdir
  local bin_dir
  workdir="$(mktemp -d)"
  trap "rm -rf -- '$workdir'" EXIT
  bin_dir="$workdir/bin"
  make_stubs "$bin_dir"

  test_runtime_renderer "$workdir" "$bin_dir"
  test_shared_lock_launcher "$workdir" "$bin_dir"
  test_compositor_dpms_and_context "$workdir" "$bin_dir"

  printf 'ok - session lock integration tests passed\n'
}

main "$@"
