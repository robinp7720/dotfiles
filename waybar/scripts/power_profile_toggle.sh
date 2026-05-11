#!/usr/bin/env bash
set -euo pipefail

# Query or toggle power-profilesctl for Waybar
PPCTL=powerprofilesctl

print_hidden() {
  printf '{"text":"","tooltip":"%s","class":"hidden"}\n' "$1"
}

if ! command -v "$PPCTL" >/dev/null 2>&1; then
  print_hidden "powerprofilesctl not installed"
  exit 0
fi

if ! current="$("$PPCTL" get 2>/dev/null)"; then
  print_hidden "Power profile unavailable"
  exit 0
fi

if [[ -z "$current" ]]; then
  print_hidden "Power profile unavailable"
  exit 0
fi

list_output="$("$PPCTL" list 2>/dev/null || true)"
readarray -t profiles < <(printf '%s\n' "$list_output" | awk '/^[* ] *(balanced|power-saver|performance)/ {name=$1=="*"?$2:$1; gsub(":","",name); print name}')

next="$current"
if [[ ${1:-} == "--toggle" ]]; then
  if [[ ${#profiles[@]} -gt 0 ]]; then
    for i in "${!profiles[@]}"; do
      if [[ "${profiles[i]}" == "$current" ]]; then
        next_index=$(((i + 1) % ${#profiles[@]}))
        next="${profiles[next_index]}"
        break
      fi
    done
  fi

  if [[ "$next" != "$current" ]]; then
    "$PPCTL" set "$next" >/dev/null 2>&1 || exit 0
    current="$("$PPCTL" get 2>/dev/null || printf '%s' "$next")"
  fi
fi

icon=""
case "$current" in
  performance) icon="" ;;
  balanced) icon="" ;;
  power-saver) icon="" ;;
esac

tooltip="Power profile: $current\\nClick to cycle"
printf '{"text":"%s","tooltip":"%s"}\n' "$icon" "$tooltip"
