#!/usr/bin/env bash
# Print a short "next event" line for Hyprlock/waybar. Falls back gracefully if no calendar CLI is configured.

set -euo pipefail

# Simple cache to avoid slow calendar CLI calls
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/next-event"
CACHE_FILE="$CACHE_DIR/next_event.json"
CACHE_TTL_SECS=120
OUTPUT_MODE="text"
BLANK_WHEN_EMPTY=0
NO_EVENT_TEXT="No upcoming events"
CALENDAR_BACKEND="${CALENDAR_BACKEND:-auto}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --waybar)
      OUTPUT_MODE="waybar"
      ;;
    --json)
      OUTPUT_MODE="json"
      ;;
    --blank-when-empty)
      BLANK_WHEN_EMPTY=1
      ;;
  esac
  shift
done

mtime_epoch() {
  local path="$1"

  stat -c %Y "$path" 2>/dev/null || stat -f %m "$path" 2>/dev/null || echo 0
}

write_cache() {
  local value="$1"
  local tmp

  mkdir -p "$CACHE_DIR"
  tmp="$(mktemp "$CACHE_DIR/next_event.XXXXXX")"
  printf '%s\n' "$value" > "$tmp"
  mv "$tmp" "$CACHE_FILE"
}

record_empty_json() {
  python3 - <<'PY'
import json

print(json.dumps({"healthy": True, "empty": True}, separators=(",", ":")))
PY
}

record_error_json() {
  local error_message="$1"

  python3 - "$error_message" <<'PY'
import json
import sys

print(json.dumps({"healthy": False, "error": sys.argv[1]}, ensure_ascii=False, separators=(",", ":")))
PY
}

render_cached_json() {
  local json="$1"

  if [[ "$OUTPUT_MODE" == "json" ]]; then
    printf '%s\n' "$json"
    exit 0
  fi

  CALENDAR_JSON="$json" python3 - "$OUTPUT_MODE" "$BLANK_WHEN_EMPTY" "$NO_EVENT_TEXT" <<'PY'
import json
import os
import sys

mode = sys.argv[1]
blank_when_empty = sys.argv[2] == "1"
no_event_text = sys.argv[3]
record = json.loads(os.environ["CALENDAR_JSON"])

healthy = bool(record.get("healthy"))
empty = bool(record.get("empty"))
text = record.get("text") or no_event_text

if not healthy or empty:
    if mode == "waybar":
        print(json.dumps({"text": "", "tooltip": "", "class": "hidden"}, separators=(",", ":")))
    elif blank_when_empty:
        print("")
    else:
        print(no_event_text)
    raise SystemExit(0)

if mode == "waybar":
    print(
        json.dumps(
            {"text": f"  {text}", "tooltip": text, "class": "has-event"},
            separators=(",", ":"),
        )
    )
else:
    print(text)
PY
  exit 0
}

current_epoch() {
  if [[ -n "${NEXT_EVENT_NOW_EPOCH:-}" ]]; then
    printf '%s\n' "$NEXT_EVENT_NOW_EPOCH"
  else
    date +%s
  fi
}

try_khal() {
  if ! command -v khal >/dev/null 2>&1; then
    return 1
  fi

  local output parsed
  if ! output="$(
    LC_ALL=C khal list \
      --format '{start-long-full}{tab}{end-long-full}{tab}{uid}{tab}{title}{tab}{location}' \
      --day-format '' \
      --once \
      now 7d 2>/dev/null
  )"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "khal command failed")"
    return 0
  fi

  if [[ -z "$output" ]]; then
    FETCH_RESULT_KIND="empty"
    FETCH_RESULT_JSON="$(record_empty_json)"
    return 0
  fi

  if ! parsed="$(KHAL_OUTPUT="$output" python3 - 2>/dev/null <<'PY'
import csv
import io
import json
import os
import subprocess


def epoch(value):
    result = subprocess.run(
        ["date", "-d", value, "+%s"],
        check=True,
        capture_output=True,
        text=True,
    )
    return int(result.stdout.strip())


rows = csv.reader(io.StringIO(os.environ["KHAL_OUTPUT"]), delimiter="\t")
row = next(rows)
if len(row) != 5:
    raise ValueError(f"expected 5 khal fields, got {len(row)}")

start_value, end_value, uid, title, location = row
start_epoch = epoch(start_value)
end_epoch = epoch(end_value) if end_value else start_epoch
event_id = uid or f"khal:{start_epoch}:{title}"
text = title
if location:
    text = f"{text} at {location}"

print("event")
print(
    json.dumps(
        {
            "healthy": True,
            "id": event_id,
            "title": title,
            "location": location or None,
            "start_epoch": start_epoch,
            "end_epoch": end_epoch,
            "text": text,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
)
PY
)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse khal output")"
    return 0
  fi

  FETCH_RESULT_KIND="${parsed%%$'\n'*}"
  FETCH_RESULT_JSON="${parsed#*$'\n'}"
  return 0
}

try_gcalcli() {
  if ! command -v gcalcli >/dev/null 2>&1; then
    return 1
  fi

  local output parsed
  if ! output="$(gcalcli --nocolor agenda --tsv --details=location now 7d 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "gcalcli command failed")"
    return 0
  fi

  if ! parsed="$(GCALCLI_OUTPUT="$output" python3 - 2>/dev/null <<'PY'
import csv
import io
import json
import os
import subprocess
import time


def epoch(date_value, time_value):
    result = subprocess.run(
        ["date", "-d", f"{date_value} {time_value}", "+%s"],
        check=True,
        capture_output=True,
        text=True,
    )
    return int(result.stdout.strip())


def format_eta(start_epoch):
    now_epoch = int(os.environ.get("NEXT_EVENT_NOW_EPOCH", time.time()))
    minutes = (max(start_epoch - now_epoch, 0) + 59) // 60
    if minutes >= 90:
        hours, remaining = divmod(minutes, 60)
        return f"{hours}h" if remaining == 0 else f"{hours}h {remaining}m"
    return f"{minutes} mins"


rows = csv.reader(io.StringIO(os.environ["GCALCLI_OUTPUT"]), delimiter="\t")
next(rows, None)
row = next(rows, None)
if row is None:
    print("empty")
    print(json.dumps({"healthy": True, "empty": True}, separators=(",", ":")))
    raise SystemExit(0)
if len(row) < 5:
    raise ValueError(f"expected at least 5 gcalcli fields, got {len(row)}")

row.extend([""] * (6 - len(row)))
start_date, start_time, end_date, end_time, summary, location = row[:6]
if not start_time:
    raise ValueError("gcalcli start time is missing")
start_epoch = epoch(start_date, start_time)
if end_time:
    end_epoch = epoch(end_date or start_date, end_time)
else:
    end_epoch = start_epoch

title = summary.split(",", 1)[0]
text = f"{title} in {format_eta(start_epoch)}"
if location:
    text = f"{text} at {location}"

print("event")
print(
    json.dumps(
        {
            "healthy": True,
            "id": f"gcalcli:{start_epoch}:{title}",
            "title": title,
            "location": location or None,
            "start_epoch": start_epoch,
            "end_epoch": end_epoch,
            "text": text,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
)
PY
)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse gcalcli output")"
    return 0
  fi

  FETCH_RESULT_KIND="${parsed%%$'\n'*}"
  FETCH_RESULT_JSON="${parsed#*$'\n'}"
  return 0
}

fetch_calendar_json() {
  local khal_empty_json=""
  local khal_error_json=""

  case "$CALENDAR_BACKEND" in
    auto)
      if try_khal; then
        if [[ "$FETCH_RESULT_KIND" == "event" ]]; then
          printf '%s\n' "$FETCH_RESULT_JSON"
          return 0
        fi
        if [[ "$FETCH_RESULT_KIND" == "empty" ]]; then
          khal_empty_json="$FETCH_RESULT_JSON"
        else
          khal_error_json="$FETCH_RESULT_JSON"
        fi
      fi

      if try_gcalcli; then
        printf '%s\n' "$FETCH_RESULT_JSON"
        return 0
      fi

      if [[ -n "$khal_empty_json" ]]; then
        printf '%s\n' "$khal_empty_json"
      elif [[ -n "$khal_error_json" ]]; then
        printf '%s\n' "$khal_error_json"
      else
        record_error_json "no supported calendar backend found"
      fi
      ;;
    khal)
      if try_khal; then
        printf '%s\n' "$FETCH_RESULT_JSON"
      else
        record_error_json "khal backend requested but not available"
      fi
      ;;
    gcalcli)
      if try_gcalcli; then
        printf '%s\n' "$FETCH_RESULT_JSON"
      else
        record_error_json "gcalcli backend requested but not available"
      fi
      ;;
    *)
      record_error_json "unsupported calendar backend: $CALENDAR_BACKEND"
      ;;
  esac
}

now_epoch="$(current_epoch)"

if [ -f "$CACHE_FILE" ]; then
  cache_mtime="$(mtime_epoch "$CACHE_FILE")"
  age=$(( now_epoch - cache_mtime ))
  if [ "$age" -lt "$CACHE_TTL_SECS" ] && [ -s "$CACHE_FILE" ]; then
    render_cached_json "$(cat "$CACHE_FILE")"
  fi
fi

calendar_json="$(fetch_calendar_json)"
write_cache "$calendar_json"
render_cached_json "$calendar_json"
