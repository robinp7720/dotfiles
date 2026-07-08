#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_UNDER_TEST="$ROOT_DIR/scripts/next_event.sh"

fail() {
  printf 'not ok - %s\n' "$*" >&2
  exit 1
}

assert_json_field() {
  local json="$1"
  local field="$2"
  local expected="$3"

  CALENDAR_JSON="$json" python3 - "$field" "$expected" <<'PY'
import json
import os
import sys

field, expected = sys.argv[1], sys.argv[2]
value = json.loads(os.environ["CALENDAR_JSON"]).get(field)
if str(value) != expected:
    raise SystemExit(f"expected {field}={expected!r}, got {value!r}")
PY
}

assert_json_bool() {
  local json="$1"
  local field="$2"
  local expected="$3"

  CALENDAR_JSON="$json" python3 - "$field" "$expected" <<'PY'
import json
import os
import sys

field = sys.argv[1]
expected = sys.argv[2].lower() == "true"
value = json.loads(os.environ["CALENDAR_JSON"]).get(field)
if value is not expected:
    raise SystemExit(f"expected {field}={expected!r}, got {value!r}")
PY
}

assert_waybar_payload() {
  local json="$1"
  local expected_text="$2"

  CALENDAR_JSON="$json" python3 - "$expected_text" <<'PY'
import json
import os
import sys

expected_text = sys.argv[1]
payload = json.loads(os.environ["CALENDAR_JSON"])
if payload.get("class") != "has-event":
    raise SystemExit(f"expected class has-event, got {payload.get('class')!r}")
if payload.get("text") != expected_text:
    raise SystemExit(f"expected text {expected_text!r}, got {payload.get('text')!r}")
if payload.get("tooltip") != expected_text.removeprefix("  "):
    raise SystemExit(f"expected tooltip {expected_text.removeprefix('  ')!r}, got {payload.get('tooltip')!r}")
PY
}

run_script() {
  local workdir="$1"
  local mode="$2"

  HOME="$workdir/home" \
  PATH="$workdir/bin:$PATH" \
  XDG_CACHE_HOME="$workdir/cache" \
  CALENDAR_BACKEND="gcalcli" \
  NEXT_EVENT_NOW_EPOCH="1800000000" \
    "$SCRIPT_UNDER_TEST" "$mode"
}

test_gcalcli_json_and_waybar_contract() {
  local workdir="$1"
  local fake_gcalcli="$workdir/bin/gcalcli"
  local json_output waybar_output cache_file

  mkdir -p "$workdir/bin" "$workdir/home"
  cat >"$fake_gcalcli" <<'FAKE_GCALCLI'
#!/usr/bin/env bash
cat <<'EOF'
start_date	start_time	end_date	end_time	summary	location
2027-01-15	09:10	2027-01-15	09:40	Design review, Engineering	Room 2
EOF
FAKE_GCALCLI
  chmod +x "$fake_gcalcli"

  json_output="$(run_script "$workdir" --json)"
  assert_json_field "$json_output" "id" "gcalcli:1800000600:Design review"
  assert_json_field "$json_output" "title" "Design review"
  assert_json_field "$json_output" "start_epoch" "1800000600"
  assert_json_field "$json_output" "end_epoch" "1800002400"
  assert_json_field "$json_output" "location" "Room 2"
  assert_json_bool "$json_output" "healthy" "true"

  cache_file="$workdir/cache/next-event/next_event.json"
  [[ -f "$cache_file" ]] || fail "expected calendar cache to be written"
  assert_json_field "$(cat "$cache_file")" "title" "Design review"

  waybar_output="$(run_script "$workdir" --waybar)"
  assert_waybar_payload "$waybar_output" "  Design review in 10 mins at Room 2"
}

main() {
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT

  test_gcalcli_json_and_waybar_contract "$tmpdir"

  printf 'ok - next_event.sh structured calendar contract passed\n'
}

main "$@"
