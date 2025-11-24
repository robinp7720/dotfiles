#!/usr/bin/env bash
# Print a short "next event" line for Hyprlock/waybar. Falls back gracefully if no calendar CLI is configured.

set -euo pipefail

# Simple cache to avoid slow calendar CLI calls
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/next-event"
CACHE_FILE="$CACHE_DIR/next_event.txt"
CACHE_TTL_SECS=120

now_epoch=$(date +%s)

if [ -f "$CACHE_FILE" ]; then
  # shellcheck disable=SC2012
  mtime_epoch=$(stat -c %Y "$CACHE_FILE" 2>/dev/null || echo 0)
  age=$(( now_epoch - mtime_epoch ))
  if [ $age -lt $CACHE_TTL_SECS ] && [ -s "$CACHE_FILE" ]; then
    cat "$CACHE_FILE"
    exit 0
  fi
fi

# Ensure cache dir exists
mkdir -p "$CACHE_DIR"

# Try khal first (commonly used with vdirsyncer)
if command -v khal >/dev/null 2>&1; then
  # 'khal list' prints lines like: 2025-11-24 09:00-10:00  Event name
  line="$(khal list now 7d 2>/dev/null | sed -n '1p')"
  if [ -n "$line" ]; then
    printf "%s" "$line"
    exit 0
  fi
fi

# Fallback to gcalcli (Google Calendar CLI)
if command -v gcalcli >/dev/null 2>&1; then
  # --tsv with details=location columns: start_date start_time end_date end_time summary location
  line="$(gcalcli --nocolor agenda --tsv --details=location now 7d 2>/dev/null | awk -F '\t' 'NR>1 && NF>=5 {print; exit}')"
  if [ -n "${line:-}" ]; then
    IFS=$'\t' read -r start_date start_time end_date end_time summary location <<<"$line"
    # Compute time until start
    if start_epoch=$(date -d "$start_date $start_time" +%s 2>/dev/null); then
      now_epoch=$(date +%s)
      diff=$(( start_epoch - now_epoch ))
      [ $diff -lt 0 ] && diff=0
      mins=$(( (diff + 59) / 60 )) # round up to next minute
      if [ $mins -ge 90 ]; then
        hours=$(( mins / 60 ))
        rem_mins=$(( mins % 60 ))
        if [ $rem_mins -eq 0 ]; then
          eta="${hours}h"
        else
          eta="${hours}h ${rem_mins}m"
        fi
      else
        eta="${mins} mins"
      fi
    else
      eta="soon"
    fi

    out="${summary}"
    if [ -n "${eta:-}" ]; then
      out="${out} in ${eta}"
    fi
    if [ -n "${location:-}" ]; then
      out="${out} at ${location}"
    fi

    printf "%s" "$out" | tee "$CACHE_FILE"
    exit 0
  fi
fi

echo "No upcoming events" | tee "$CACHE_FILE"
