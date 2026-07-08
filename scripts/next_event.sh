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

json_escape() {
  local s="$1"
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\n'/\\n}
  printf '%s' "$s"
}

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

json_string_or_null() {
  local value="${1:-}"

  if [[ -n "$value" ]]; then
    printf '"%s"' "$(json_escape "$value")"
  else
    printf 'null'
  fi
}

json_number_or_null() {
  local value="${1:-}"

  if [[ -n "$value" ]]; then
    printf '%s' "$value"
  else
    printf 'null'
  fi
}

record_event_json() {
  local id="$1"
  local title="$2"
  local location="${3:-}"
  local start_epoch="$4"
  local end_epoch="${5:-}"
  local text="$6"

  printf '{"healthy":true,"id":"%s","title":"%s","location":%s,"start_epoch":%s,"end_epoch":%s,"text":"%s"}\n' \
    "$(json_escape "$id")" \
    "$(json_escape "$title")" \
    "$(json_string_or_null "$location")" \
    "$start_epoch" \
    "$(json_number_or_null "$end_epoch")" \
    "$(json_escape "$text")"
}

record_empty_json() {
  printf '{"healthy":true,"empty":true}\n'
}

record_error_json() {
  local error_message="$1"

  printf '{"healthy":false,"error":"%s"}\n' "$(json_escape "$error_message")"
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

format_eta() {
  local start_epoch="$1"
  local now_epoch diff mins hours rem_mins

  now_epoch="$(current_epoch)"
  diff=$(( start_epoch - now_epoch ))
  (( diff < 0 )) && diff=0
  mins=$(( (diff + 59) / 60 ))

  if (( mins >= 90 )); then
    hours=$(( mins / 60 ))
    rem_mins=$(( mins % 60 ))
    if (( rem_mins == 0 )); then
      printf '%sh\n' "$hours"
    else
      printf '%sh %sm\n' "$hours" "$rem_mins"
    fi
    return
  fi

  printf '%s mins\n' "$mins"
}

try_khal() {
  if ! command -v khal >/dev/null 2>&1; then
    return 1
  fi

  local line event_date start_time end_time title start_epoch end_epoch id
  if ! line="$(khal list now 7d 2>/dev/null | sed -n '1p')"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "khal command failed")"
    return 0
  fi

  if [[ -z "$line" ]]; then
    FETCH_RESULT_KIND="empty"
    FETCH_RESULT_JSON="$(record_empty_json)"
    return 0
  fi

  if [[ "$line" =~ ^([0-9]{4}-[0-9]{2}-[0-9]{2})[[:space:]]+([0-9]{2}:[0-9]{2})(-([0-9]{2}:[0-9]{2}))?[[:space:]]{2,}(.*)$ ]]; then
    event_date="${BASH_REMATCH[1]}"
    start_time="${BASH_REMATCH[2]}"
    end_time="${BASH_REMATCH[4]:-}"
    title="${BASH_REMATCH[5]}"
  else
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse khal output")"
    return 0
  fi

  if ! start_epoch="$(date -d "$event_date $start_time" +%s 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse khal start time")"
    return 0
  fi

  end_epoch=""
  if [[ -n "$end_time" ]] && ! end_epoch="$(date -d "$event_date $end_time" +%s 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse khal end time")"
    return 0
  fi

  id="khal:${start_epoch}:${title}"
  FETCH_RESULT_KIND="event"
  FETCH_RESULT_JSON="$(record_event_json "$id" "$title" "" "$start_epoch" "$end_epoch" "$line")"
  return 0
}

try_gcalcli() {
  if ! command -v gcalcli >/dev/null 2>&1; then
    return 1
  fi

  local line start_date start_time end_date end_time summary location start_epoch end_epoch eta out id
  if ! line="$(
    gcalcli --nocolor agenda --tsv --details=location now 7d 2>/dev/null |
      awk -F '\t' 'NR>1 && NF>=5 {
        printf "%s\x1f%s\x1f%s\x1f%s\x1f%s\x1f%s\n", $1, $2, $3, $4, $5, $6
        exit
      }'
  )"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "gcalcli command failed")"
    return 0
  fi

  if [[ -z "${line:-}" ]]; then
    FETCH_RESULT_KIND="empty"
    FETCH_RESULT_JSON="$(record_empty_json)"
    return 0
  fi

  IFS=$'\x1f' read -r start_date start_time end_date end_time summary location <<<"$line"
  if [[ -z "${start_time:-}" ]] || ! start_epoch="$(date -d "$start_date $start_time" +%s 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse gcalcli start time")"
    return 0
  fi

  end_epoch=""
  if [[ -n "${end_time:-}" ]] && ! end_epoch="$(date -d "$end_date $end_time" +%s 2>/dev/null)"; then
    FETCH_RESULT_KIND="error"
    FETCH_RESULT_JSON="$(record_error_json "failed to parse gcalcli end time")"
    return 0
  fi

  eta="$(format_eta "$start_epoch")"
  summary="${summary%%,*}"

  out="${summary}"
  if [[ -n "${eta:-}" ]]; then
    out="${out} in ${eta}"
  fi
  if [[ -n "${location:-}" ]]; then
    out="${out} at ${location}"
  fi

  id="gcalcli:${start_epoch}:${summary}"
  FETCH_RESULT_KIND="event"
  FETCH_RESULT_JSON="$(record_event_json "$id" "$summary" "${location:-}" "$start_epoch" "$end_epoch" "$out")"
  return 0
}

fetch_calendar_json() {
  local khal_empty_json=""

  case "$CALENDAR_BACKEND" in
    auto)
      if try_khal; then
        if [[ "$FETCH_RESULT_KIND" != "empty" ]]; then
          printf '%s\n' "$FETCH_RESULT_JSON"
          return 0
        fi
        khal_empty_json="$FETCH_RESULT_JSON"
      fi

      if try_gcalcli; then
        printf '%s\n' "$FETCH_RESULT_JSON"
        return 0
      fi

      if [[ -n "$khal_empty_json" ]]; then
        printf '%s\n' "$khal_empty_json"
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
