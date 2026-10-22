#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_UNDER_TEST="$ROOT_DIR/scripts/night_light.sh"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

main() {
  local tmpdir fake_bin
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT
  fake_bin="$tmpdir/bin"
  mkdir -p "$fake_bin"

  # A fake hyprsunset IPC that remembers the temperature in a file.
  cat >"$fake_bin/hyprctl" <<'FAKE_HYPRCTL'
#!/usr/bin/env bash
state="$FAKE_KELVIN_FILE"
case "$*" in
  "hyprsunset temperature") cat "$state" ;;
  "hyprsunset temperature "*) printf '%s\n' "$3" >"$state"; echo ok ;;
  "hyprsunset identity") echo 6000 >"$state"; echo ok ;;
  *) exit 1 ;;
esac
FAKE_HYPRCTL
  chmod +x "$fake_bin/hyprctl"
  export FAKE_KELVIN_FILE="$tmpdir/kelvin"
  echo 6000 >"$FAKE_KELVIN_FILE"

  run() { PATH="$fake_bin:$PATH" "$SCRIPT_UNDER_TEST" "$@"; }

  [[ "$(run status)" == off ]] || fail "neutral temperature should report off"
  run toggle
  [[ "$(cat "$FAKE_KELVIN_FILE")" == 4000 ]] || fail "toggle should warm the screen"
  [[ "$(run status)" == on ]] || fail "a warm temperature should report on"
  run toggle
  [[ "$(cat "$FAKE_KELVIN_FILE")" == 6000 ]] || fail "toggle should restore neutral colors"
  NIGHT_LIGHT_KELVIN=3500 run on
  [[ "$(cat "$FAKE_KELVIN_FILE")" == 3500 ]] || fail "on should honor NIGHT_LIGHT_KELVIN"
  run off
  [[ "$(run status)" == off ]] || fail "off should restore neutral colors"
  if run bogus 2>/dev/null; then
    fail "unknown commands should fail"
  fi

  printf 'ok - night light tests passed\n'
}

main "$@"
