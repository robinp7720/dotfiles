#!/usr/bin/env bash
# Structured "next event" for Eww: multi-line output with summary, time span, ETA, location.

set -euo pipefail

format_eta() {
  local start_epoch="$1"
  local now_epoch
  now_epoch=$(date +%s)
  local diff=$(( start_epoch - now_epoch ))
  if (( diff <= 0 )); then
    echo "now"
    return
  fi
  local mins=$(( (diff + 59) / 60 ))
  if (( mins >= 90 )); then
    local hours=$(( mins / 60 ))
    local rem=$(( mins % 60 ))
    if (( rem == 0 )); then
      echo "${hours}h"
    else
      echo "${hours}h ${rem}m"
    fi
  else
    echo "${mins}m"
  fi
}

format_day_label() {
  local date_str="$1" # YYYY-MM-DD
  local today tomorrow
  today=$(date +%Y-%m-%d)
  tomorrow=$(date -d "+1 day" +%Y-%m-%d)
  if [[ "$date_str" == "$today" ]]; then
    echo "Today"
  elif [[ "$date_str" == "$tomorrow" ]]; then
    echo "Tomorrow"
  else
    date -d "$date_str" "+%a, %b %-d"
  fi
}

output() {
  local summary="$1" start_date="$2" start_time="$3" end_time="$4" location="$5"
  local start_epoch
  start_epoch=$(date -d "$start_date $start_time" +%s 2>/dev/null || echo 0)

  local eta=""
  if [[ "$start_epoch" -gt 0 ]]; then
    eta=$(format_eta "$start_epoch")
  fi

  local day_label
  day_label=$(format_day_label "$start_date")

  printf "%s\n" "$summary"
  printf "%s  %s – %s\n" "$day_label" "$start_time" "$end_time"
  if [[ -n "$eta" ]]; then
    printf "Starts in %s\n" "$eta"
  fi
  if [[ -n "$location" ]]; then
    printf "%s\n" "$location"
  fi
  exit 0
}

# Try khal first
if command -v khal >/dev/null 2>&1; then
  # khal list: "2025-11-24 09:00-10:00  Event name"
  line="$(khal list now 7d 2>/dev/null | sed -n '1p')"
  if [[ -n "$line" ]]; then
    read -r date times rest <<<"$line"
    start_time="${times%-*}"
    end_time="${times#*-}"
    summary="$rest"
    output "$summary" "$date" "$start_time" "$end_time" ""
  fi
fi

# Fallback gcalcli
if command -v gcalcli >/dev/null 2>&1; then
  # tsv: start_date start_time end_date end_time summary location
  line="$(gcalcli --nocolor agenda --tsv --details=location now 7d 2>/dev/null | awk -F '\t' 'NR>1 && NF>=5 {print; exit}')"
  if [[ -n "$line" ]]; then
    IFS=$'\t' read -r start_date start_time end_date end_time summary location <<<"$line"
    output "$summary" "$start_date" "$start_time" "$end_time" "${location:-}"
  fi
fi

echo "No upcoming events"
