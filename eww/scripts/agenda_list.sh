#!/usr/bin/env bash
# Print a short agenda (up to 5 upcoming events) for Eww.

set -euo pipefail

MAX_ITEMS=5

format_day_label() {
  local date_str="$1"
  local today tomorrow
  today=$(date +%Y-%m-%d)
  tomorrow=$(date -d "+1 day" +%Y-%m-%d)
  if [[ "$date_str" == "$today" ]]; then
    echo "Today"
  elif [[ "$date_str" == "$tomorrow" ]]; then
    echo "Tomorrow"
  else
    date -d "$date_str" "+%a %b %-d"
  fi
}

print_agenda() {
  local count=0
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    read -r date times rest <<<"$line"
    start_time="${times%-*}"
    end_time="${times#*-}"
    day_label=$(format_day_label "$date")
    printf "%s  %s – %s\n%s\n\n" "$day_label" "$start_time" "$end_time" "$rest"
    count=$((count+1))
    (( count >= MAX_ITEMS )) && break
  done
  exit 0
}

# khal path
if command -v khal >/dev/null 2>&1; then
  # next 24h agenda
  agenda_lines=$(khal list now 2d 2>/dev/null | head -n 20)
  if [[ -n "$agenda_lines" ]]; then
    print_agenda <<<"$agenda_lines"
  fi
fi

# gcalcli fallback
if command -v gcalcli >/dev/null 2>&1; then
  agenda_lines=$(gcalcli --nocolor agenda --tsv --details=location now 2d 2>/dev/null | tail -n +2 | head -n 20)
  if [[ -n "$agenda_lines" ]]; then
    count=0
    while IFS=$'\t' read -r start_date start_time end_date end_time summary location; do
      (( count >= MAX_ITEMS )) && break
      [[ -z "$summary" ]] && continue
      day_label=$(format_day_label "$start_date")
      loc=""
      [[ -n "$location" ]] && loc="\n$location"
      printf "%s  %s – %s\n%s%s\n\n" "$day_label" "$start_time" "$end_time" "$summary" "$loc"
      count=$((count+1))
    done <<<"$agenda_lines"
    exit 0
  fi
fi

echo "No upcoming events" 
