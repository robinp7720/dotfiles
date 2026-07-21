#!/usr/bin/env bash
# Print a short next-event line or a structured calendar agenda.

set -euo pipefail

CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/next-event"
CACHE_TTL_SECS=120
OUTPUT_MODE="text"
BLANK_WHEN_EMPTY=0
NO_EVENT_TEXT="No upcoming events"
CALENDAR_BACKEND="${CALENDAR_BACKEND:-auto}"
QUERY_FROM=""
QUERY_TO=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --waybar)
      OUTPUT_MODE="waybar"
      shift
      ;;
    --json)
      OUTPUT_MODE="json"
      shift
      ;;
    --agenda-json)
      OUTPUT_MODE="agenda_json"
      shift
      ;;
    --from)
      [[ $# -ge 2 ]] || { printf '%s\n' "--from requires a date" >&2; exit 2; }
      QUERY_FROM="$2"
      shift 2
      ;;
    --to)
      [[ $# -ge 2 ]] || { printf '%s\n' "--to requires a date" >&2; exit 2; }
      QUERY_TO="$2"
      shift 2
      ;;
    --blank-when-empty)
      BLANK_WHEN_EMPTY=1
      shift
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ "$OUTPUT_MODE" == "agenda_json" ]]; then
  [[ -n "$QUERY_FROM" && -n "$QUERY_TO" ]] || {
    printf '%s\n' "--agenda-json requires --from and --to" >&2
    exit 2
  }
  date -d "$QUERY_FROM" +%F >/dev/null 2>&1 || { printf 'invalid --from date: %s\n' "$QUERY_FROM" >&2; exit 2; }
  date -d "$QUERY_TO" +%F >/dev/null 2>&1 || { printf 'invalid --to date: %s\n' "$QUERY_TO" >&2; exit 2; }
  QUERY_FROM="$(date -d "$QUERY_FROM" +%F)"
  QUERY_TO="$(date -d "$QUERY_TO" +%F)"
  [[ "$(date -d "$QUERY_TO" +%s)" -gt "$(date -d "$QUERY_FROM" +%s)" ]] || {
    printf '%s\n' "--to must be after --from" >&2
    exit 2
  }
  CACHE_FILE="$CACHE_DIR/agenda_${QUERY_FROM}_${QUERY_TO}.json"
else
  CACHE_FILE="$CACHE_DIR/next_event.json"
fi

mtime_epoch() {
  local path="$1"
  stat -c %Y "$path" 2>/dev/null || stat -f %m "$path" 2>/dev/null || echo 0
}

write_cache() {
  local value="$1"
  local tmp
  mkdir -p "$CACHE_DIR"
  tmp="$(mktemp "$CACHE_DIR/calendar.XXXXXX")"
  printf '%s\n' "$value" > "$tmp"
  mv "$tmp" "$CACHE_FILE"
}

record_empty_json() {
  if [[ "$OUTPUT_MODE" == "agenda_json" ]]; then
    python3 - "$QUERY_FROM" "$QUERY_TO" <<'PY'
import json
import sys
print(json.dumps({"healthy": True, "empty": True, "range_start": sys.argv[1], "range_end": sys.argv[2], "events": []}, separators=(",", ":")))
PY
  else
    printf '%s\n' '{"healthy":true,"empty":true}'
  fi
}

record_error_json() {
  local error_message="$1"
  python3 - "$error_message" "$OUTPUT_MODE" "$QUERY_FROM" "$QUERY_TO" <<'PY'
import json
import sys
payload = {"healthy": False, "error": sys.argv[1]}
if sys.argv[2] == "agenda_json":
    payload.update({"range_start": sys.argv[3], "range_end": sys.argv[4], "events": []})
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
PY
}

render_cached_json() {
  local json="$1"
  if [[ "$OUTPUT_MODE" == "json" || "$OUTPUT_MODE" == "agenda_json" ]]; then
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
    print(json.dumps({"text": f"  {text}", "tooltip": text, "class": "has-event"}, separators=(",", ":")))
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
  command -v khal >/dev/null 2>&1 || return 1
  local output parsed range_start range_end
  if [[ "$OUTPUT_MODE" == "agenda_json" ]]; then
    range_start="$QUERY_FROM"
    range_end="$QUERY_TO"
  else
    range_start="now"
    range_end="7d"
  fi
  if ! output="$(
    LC_ALL=C khal list \
      --format '{start-long-full}{tab}{end-long-full}{tab}{uid}{tab}{title}{tab}{location}' \
      --day-format '' \
      --once \
      "$range_start" "$range_end" 2>/dev/null
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

  if ! parsed="$(KHAL_OUTPUT="$output" python3 - "$OUTPUT_MODE" "$QUERY_FROM" "$QUERY_TO" 2>/dev/null <<'PY'
import csv
import io
import json
import os
import subprocess
import sys

mode, range_start, range_end = sys.argv[1:4]
def epoch(value):
    return int(subprocess.run(["date", "-d", value, "+%s"], check=True, capture_output=True, text=True).stdout.strip())

events = []
for row in csv.reader(io.StringIO(os.environ["KHAL_OUTPUT"]), delimiter="\t"):
    if len(row) != 5:
        raise ValueError(f"expected 5 khal fields, got {len(row)}")
    start_value, end_value, uid, title, location = row
    start_epoch = epoch(start_value)
    end_epoch = epoch(end_value) if end_value else start_epoch
    event = {
        "id": uid or f"khal:{start_epoch}:{title}",
        "title": title,
        "location": location or None,
        "calendar": None,
        "start_epoch": start_epoch,
        "end_epoch": end_epoch,
        "all_day": start_value.endswith(" 00:00") and end_epoch - start_epoch >= 86400,
    }
    events.append(event)

events = sorted({event["id"]: event for event in events}.values(), key=lambda event: (event["start_epoch"], event["title"], event["id"]))
if mode == "agenda_json":
    print("event" if events else "empty")
    print(json.dumps({"healthy": True, "empty": not events, "range_start": range_start, "range_end": range_end, "events": events}, ensure_ascii=False, separators=(",", ":")))
else:
    event = events[0]
    text = event["title"] + (f" at {event['location']}" if event["location"] else "")
    event.update({"healthy": True, "text": text})
    event.pop("calendar", None)
    event.pop("all_day", None)
    print("event")
    print(json.dumps(event, ensure_ascii=False, separators=(",", ":")))
PY
)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse khal output")"
    return 0
  fi
  FETCH_RESULT_KIND="${parsed%%$'\n'*}"
  FETCH_RESULT_JSON="${parsed#*$'\n'}"
}

try_gcalcli() {
  command -v gcalcli >/dev/null 2>&1 || return 1
  local output parsed range_start range_end
  if [[ "$OUTPUT_MODE" == "agenda_json" ]]; then
    range_start="$QUERY_FROM"
    range_end="$QUERY_TO"
  else
    range_start="now"
    range_end="7d"
  fi
  if ! output="$(gcalcli --nocolor agenda --tsv --details=location --details=calendar --details=id "$range_start" "$range_end" 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "gcalcli command failed")"
    return 0
  fi

  if ! parsed="$(GCALCLI_OUTPUT="$output" python3 - "$OUTPUT_MODE" "$QUERY_FROM" "$QUERY_TO" 2>/dev/null <<'PY'
import csv
import io
import json
import os
import subprocess
import sys
import time

mode, range_start, range_end = sys.argv[1:4]
def epoch(value):
    return int(subprocess.run(["date", "-d", value, "+%s"], check=True, capture_output=True, text=True).stdout.strip())
def eta(start):
    now = int(os.environ.get("NEXT_EVENT_NOW_EPOCH", time.time()))
    minutes = (max(start - now, 0) + 59) // 60
    if minutes >= 90:
        hours, rest = divmod(minutes, 60)
        return f"{hours}h" if rest == 0 else f"{hours}h {rest}m"
    return f"{minutes} mins"

rows = list(csv.reader(io.StringIO(os.environ["GCALCLI_OUTPUT"]), delimiter="\t"))
if not rows:
    events = []
else:
    header = [field.strip().lower() for field in rows[0]]
    events = []
    for row in rows[1:]:
        row += [""] * max(0, len(header) - len(row))
        values = dict(zip(header, row))
        start_date = values.get("start_date", row[0] if row else "")
        start_time = values.get("start_time", row[1] if len(row) > 1 else "")
        end_date = values.get("end_date", row[2] if len(row) > 2 else "")
        end_time = values.get("end_time", row[3] if len(row) > 3 else "")
        summary = values.get("summary", values.get("title", row[4] if len(row) > 4 else ""))
        location = values.get("location", row[5] if len(row) > 5 else "")
        calendar = values.get("calendar", "")
        backend_id = values.get("id", "")
        if not start_date:
            raise ValueError("gcalcli start date is missing")
        start_epoch = epoch(f"{start_date} {start_time}" if start_time else start_date)
        if end_time:
            end_epoch = epoch(f"{end_date or start_date} {end_time}")
        elif mode == "agenda_json" and end_date:
            end_epoch = epoch(end_date)
        elif mode == "agenda_json" and not start_time:
            end_epoch = start_epoch + 86400
        else:
            end_epoch = start_epoch
        title = summary.split(",", 1)[0]
        events.append({
            "id": backend_id or f"gcalcli:{start_epoch}:{title}",
            "title": title,
            "location": location or None,
            "calendar": calendar or None,
            "start_epoch": start_epoch,
            "end_epoch": end_epoch,
            "all_day": not bool(start_time),
        })

events = sorted({event["id"]: event for event in events}.values(), key=lambda event: (event["start_epoch"], event["title"], event["id"]))
if mode == "agenda_json":
    print("event" if events else "empty")
    print(json.dumps({"healthy": True, "empty": not events, "range_start": range_start, "range_end": range_end, "events": events}, ensure_ascii=False, separators=(",", ":")))
elif not events:
    print("empty")
    print(json.dumps({"healthy": True, "empty": True}, separators=(",", ":")))
else:
    event = events[0]
    text = event["title"]
    if not event["all_day"]:
        text += f" in {eta(event['start_epoch'])}"
    if event["location"]:
        text += f" at {event['location']}"
    event.update({"healthy": True, "text": text})
    event.pop("calendar", None)
    event.pop("all_day", None)
    print("event")
    print(json.dumps(event, ensure_ascii=False, separators=(",", ":")))
PY
)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse gcalcli output")"
    return 0
  fi
  FETCH_RESULT_KIND="${parsed%%$'\n'*}"
  FETCH_RESULT_JSON="${parsed#*$'\n'}"
}

fetch_calendar_json() {
  local khal_empty_json="" khal_error_json=""
  case "$CALENDAR_BACKEND" in
    auto)
      if try_khal; then
        if [[ "$FETCH_RESULT_KIND" == "event" ]]; then printf '%s\n' "$FETCH_RESULT_JSON"; return 0; fi
        if [[ "$FETCH_RESULT_KIND" == "empty" ]]; then khal_empty_json="$FETCH_RESULT_JSON"; else khal_error_json="$FETCH_RESULT_JSON"; fi
      fi
      if try_gcalcli; then printf '%s\n' "$FETCH_RESULT_JSON"; return 0; fi
      if [[ -n "$khal_empty_json" ]]; then printf '%s\n' "$khal_empty_json"
      elif [[ -n "$khal_error_json" ]]; then printf '%s\n' "$khal_error_json"
      else record_error_json "no supported calendar backend found"; fi
      ;;
    khal)
      if try_khal; then printf '%s\n' "$FETCH_RESULT_JSON"; else record_error_json "khal backend requested but not available"; fi
      ;;
    gcalcli)
      if try_gcalcli; then printf '%s\n' "$FETCH_RESULT_JSON"; else record_error_json "gcalcli backend requested but not available"; fi
      ;;
    *) record_error_json "unsupported calendar backend: $CALENDAR_BACKEND" ;;
  esac
}

now_epoch="$(current_epoch)"
if [[ -f "$CACHE_FILE" ]]; then
  cache_mtime="$(mtime_epoch "$CACHE_FILE")"
  age=$(( now_epoch - cache_mtime ))
  if [[ "$age" -lt "$CACHE_TTL_SECS" && -s "$CACHE_FILE" ]]; then
    render_cached_json "$(<"$CACHE_FILE")"
  fi
fi

calendar_json="$(fetch_calendar_json)"
write_cache "$calendar_json"
render_cached_json "$calendar_json"
