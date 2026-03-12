#!/usr/bin/env bash
# Print a short "next event" line for Hyprlock/waybar. Falls back gracefully if no calendar CLI is configured.

set -euo pipefail

# Simple cache to avoid slow calendar CLI calls
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/next-event"
CACHE_FILE="$CACHE_DIR/next_event.txt"
CACHE_TTL_SECS=120
OUTPUT_MODE="text"

if [[ "${1:-}" == "--waybar" ]]; then
  OUTPUT_MODE="waybar"
fi

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

print_and_exit() {
  local value="$1"

  if [[ "$OUTPUT_MODE" == "waybar" ]]; then
    local class="has-event"
    local text="  $value"

    if [[ "$value" == "No upcoming events" ]]; then
      class="hidden"
      text=""
    fi

    printf '{"text":"%s","tooltip":"%s","class":"%s"}\n' \
      "$(json_escape "$text")" \
      "$(json_escape "$value")" \
      "$class"
  else
    printf '%s\n' "$value"
  fi

  exit 0
}

format_eta() {
  local start_epoch="$1"
  local now_epoch diff mins hours rem_mins

  now_epoch=$(date +%s)
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

now_epoch="$(date +%s)"

if [ -f "$CACHE_FILE" ]; then
  cache_mtime="$(mtime_epoch "$CACHE_FILE")"
  age=$(( now_epoch - cache_mtime ))
  if [ "$age" -lt "$CACHE_TTL_SECS" ] && [ -s "$CACHE_FILE" ]; then
    print_and_exit "$(cat "$CACHE_FILE")"
  fi
fi

# Try khal first (commonly used with vdirsyncer)
if command -v khal >/dev/null 2>&1; then
  # 'khal list' prints lines like: 2025-11-24 09:00-10:00  Event name
  line="$(khal list now 7d 2>/dev/null | sed -n '1p')"
  if [ -n "$line" ]; then
    write_cache "$line"
    print_and_exit "$line"
  fi
fi

# Fallback to gcalcli (Google Calendar CLI)
if command -v gcalcli >/dev/null 2>&1; then
  # --tsv with details=location columns: start_date start_time end_date end_time summary location
  line="$(gcalcli --nocolor agenda --tsv --details=location now 7d 2>/dev/null | awk -F '\t' 'NR>1 && NF>=5 {print; exit}')"
  if [ -n "${line:-}" ]; then
    IFS=$'\t' read -r start_date start_time end_date end_time summary location <<<"$line"
    if start_epoch=$(date -d "$start_date $start_time" +%s 2>/dev/null); then
      eta="$(format_eta "$start_epoch")"
    else
      eta="soon"
    fi

    summary="${summary%%,*}"

    out="${summary}"
    if [ -n "${eta:-}" ]; then
      out="${out} in ${eta}"
    fi
    if [ -n "${location:-}" ]; then
      out="${out} at ${location}"
    fi

    write_cache "$out"
    print_and_exit "$out"
  fi
fi

write_cache "No upcoming events"
print_and_exit "No upcoming events"
