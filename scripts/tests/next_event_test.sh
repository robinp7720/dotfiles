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
  local backend="$2"
  local mode="$3"

  HOME="$workdir/home" \
  PATH="$workdir/bin:$PATH" \
  TZ="UTC" \
  XDG_CACHE_HOME="$workdir/cache" \
  CALENDAR_BACKEND="$backend" \
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
2027-01-15	08:10	2027-01-15	08:40	Design review, Engineering	Room 2
EOF
FAKE_GCALCLI
  chmod +x "$fake_gcalcli"

  json_output="$(run_script "$workdir" gcalcli --json)"
  assert_json_field "$json_output" "id" "gcalcli:1800000600:Design review"
  assert_json_field "$json_output" "title" "Design review"
  assert_json_field "$json_output" "start_epoch" "1800000600"
  assert_json_field "$json_output" "end_epoch" "1800002400"
  assert_json_field "$json_output" "location" "Room 2"
  assert_json_bool "$json_output" "healthy" "true"

  cache_file="$workdir/cache/next-event/next_event.json"
  [[ -f "$cache_file" ]] || fail "expected calendar cache to be written"
  assert_json_field "$(cat "$cache_file")" "title" "Design review"

  waybar_output="$(run_script "$workdir" gcalcli --waybar)"
  assert_waybar_payload "$waybar_output" "  Design review in 10 mins at Room 2"
}

test_auto_falls_back_after_khal_parse_failure() {
  local workdir="$1"
  local json_output

  mkdir -p "$workdir/bin" "$workdir/home"
  cat >"$workdir/bin/khal" <<'FAKE_KHAL'
#!/usr/bin/env bash
printf '%s\n' 'not a structured khal record'
FAKE_KHAL
  cat >"$workdir/bin/gcalcli" <<'FAKE_GCALCLI'
#!/usr/bin/env bash
cat <<'EOF'
start_date	start_time	end_date	end_time	summary	location
2027-01-15	08:10	2027-01-15	08:40	Fallback review	Room 3
EOF
FAKE_GCALCLI
  chmod +x "$workdir/bin/khal" "$workdir/bin/gcalcli"

  json_output="$(run_script "$workdir" auto --json)"
  assert_json_field "$json_output" "title" "Fallback review"
  assert_json_field "$json_output" "end_epoch" "1800002400"
  assert_json_bool "$json_output" "healthy" "true"
}

test_khal_all_day_event_uses_explicit_format() {
  local workdir="$1"
  local json_output

  mkdir -p "$workdir/bin" "$workdir/home"
  cat >"$workdir/bin/khal" <<'FAKE_KHAL'
#!/usr/bin/env bash
expected='{start-long-full}{tab}{end-long-full}{tab}{uid}{tab}{title}{tab}{location}'
format=''
day_format='missing'
once=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --format)
      format="$2"
      shift 2
      ;;
    --day-format)
      day_format="$2"
      shift 2
      ;;
    --once)
      once=1
      shift
      ;;
    *)
      shift
      ;;
  esac
done
[[ "$format" == "$expected" && -z "$day_format" && "$once" == 1 && "${LC_ALL:-}" == "C" ]] || exit 64
printf '2027-01-16 00:00\t2027-01-17 00:00\tall-day-uid\tConference day\tHall A\n'
FAKE_KHAL
  chmod +x "$workdir/bin/khal"

  json_output="$(run_script "$workdir" khal --json)"
  assert_json_field "$json_output" "id" "all-day-uid"
  assert_json_field "$json_output" "title" "Conference day"
  assert_json_field "$json_output" "start_epoch" "1800057600"
  assert_json_field "$json_output" "end_epoch" "1800144000"
  assert_json_field "$json_output" "location" "Hall A"
}

test_missing_backend_end_normalizes_to_start() {
  local workdir="$1"
  local json_output

  mkdir -p "$workdir/bin" "$workdir/home"
  cat >"$workdir/bin/gcalcli" <<'FAKE_GCALCLI'
#!/usr/bin/env bash
printf 'start_date\tstart_time\tend_date\tend_time\tsummary\tlocation\n'
printf '2027-01-15\t08:10\t\t\tOpen-ended review\tRoom 4\n'
FAKE_GCALCLI
  chmod +x "$workdir/bin/gcalcli"

  json_output="$(run_script "$workdir" gcalcli --json)"
  assert_json_field "$json_output" "start_epoch" "1800000600"
  assert_json_field "$json_output" "end_epoch" "1800000600"
}

test_control_characters_are_serialized_by_python_json() {
  local workdir="$1"
  local json_output title location

  title=$'Planning "A"\tB\rC\nD\\E'
  location=$'Room "2"\tEast\rWing\nDesk\\7'
  mkdir -p "$workdir/bin" "$workdir/home"
  cat >"$workdir/bin/gcalcli" <<'FAKE_GCALCLI'
#!/usr/bin/env bash
python3 - <<'PY'
import csv
import sys

writer = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
writer.writerow(["start_date", "start_time", "end_date", "end_time", "summary", "location"])
writer.writerow([
    "2027-01-15",
    "08:10",
    "2027-01-15",
    "08:40",
    'Planning "A"\tB\rC\nD\\E',
    'Room "2"\tEast\rWing\nDesk\\7',
])
PY
FAKE_GCALCLI
  chmod +x "$workdir/bin/gcalcli"

  json_output="$(run_script "$workdir" gcalcli --json)"
  assert_json_field "$json_output" "title" "$title"
  assert_json_field "$json_output" "location" "$location"
  CALENDAR_JSON="$json_output" python3 - <<'PY'
import json
import os

record = json.loads(os.environ["CALENDAR_JSON"])
assert record["end_epoch"] > 0
PY
}

main() {
  local tmpdir
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT

  case "${1:-all}" in
    gcalcli-contract)
      test_gcalcli_json_and_waybar_contract "$tmpdir/gcalcli-contract"
      ;;
    khal-fallback)
      test_auto_falls_back_after_khal_parse_failure "$tmpdir/khal-fallback"
      ;;
    khal-all-day)
      test_khal_all_day_event_uses_explicit_format "$tmpdir/khal-all-day"
      ;;
    missing-end)
      test_missing_backend_end_normalizes_to_start "$tmpdir/missing-end"
      ;;
    control-characters)
      test_control_characters_are_serialized_by_python_json "$tmpdir/control-characters"
      ;;
    all)
      test_gcalcli_json_and_waybar_contract "$tmpdir/gcalcli-contract"
      test_auto_falls_back_after_khal_parse_failure "$tmpdir/khal-fallback"
      test_khal_all_day_event_uses_explicit_format "$tmpdir/khal-all-day"
      test_missing_backend_end_normalizes_to_start "$tmpdir/missing-end"
      test_control_characters_are_serialized_by_python_json "$tmpdir/control-characters"
      ;;
    *)
      fail "unknown test case: $1"
      ;;
  esac

  printf 'ok - next_event.sh structured calendar contract passed\n'
}

main "$@"
