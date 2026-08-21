#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
WATCHER_PID=""

cleanup() {
  if [[ -n "$WATCHER_PID" ]]; then
    kill "$WATCHER_PID" 2>/dev/null || true
    wait "$WATCHER_PID" 2>/dev/null || true
  fi
  rm -rf -- "$TEMP_ROOT"
}
trap cleanup EXIT

mkdir -p "$TEMP_ROOT/bin"

cat >"$TEMP_ROOT/bin/playerctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

calls_file="$TEST_ROOT/playerctl-calls"
calls=0
[[ ! -f "$calls_file" ]] || calls="$(<"$calls_file")"
printf '%s\n' "$((calls + 1))" >"$calls_file"
printf 'metadata changed\n'
exit 1
EOF

cat >"$TEMP_ROOT/bin/now-playing" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

calls_file="$TEST_ROOT/refresh-calls"
calls=0
[[ ! -f "$calls_file" ]] || calls="$(<"$calls_file")"
printf '%s\n' "$((calls + 1))" >"$calls_file"
EOF

chmod +x "$TEMP_ROOT/bin/playerctl" "$TEMP_ROOT/bin/now-playing"

PATH="$TEMP_ROOT/bin:$PATH" \
TEST_ROOT="$TEMP_ROOT" \
NOW_PLAYING_BIN="$TEMP_ROOT/bin/now-playing" \
ALBUM_ART_WATCHER_RETRY_SECONDS=0.01 \
  "$ROOT/scripts/album_art_watcher.sh" &
WATCHER_PID=$!

for _ in {1..100}; do
  playerctl_calls=0
  refresh_calls=0
  if [[ -f "$TEMP_ROOT/playerctl-calls" ]]; then
    read -r playerctl_calls <"$TEMP_ROOT/playerctl-calls"
  fi
  if [[ -f "$TEMP_ROOT/refresh-calls" ]]; then
    read -r refresh_calls <"$TEMP_ROOT/refresh-calls"
  fi
  if ((playerctl_calls >= 2 && refresh_calls >= 4)); then
    printf 'ok - album art watcher reconnects after metadata stream exits\n'
    exit 0
  fi
  sleep 0.01
done

printf 'not ok - watcher did not reconnect after metadata stream exited\n' >&2
exit 1
