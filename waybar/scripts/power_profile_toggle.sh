#!/usr/bin/env bash
set -euo pipefail

# Query or toggle power-profilesctl for Waybar
PPCTL=powerprofilesctl

current="$($PPCTL get)"

readarray -t profiles < <($PPCTL list | awk '/^[* ] *(balanced|power-saver|performance)/ {name=$1=="*"?$2:$1; gsub(":","",name); print name}')

next="$current"
if [[ ${1:-} == "--toggle" ]]; then
    if [[ ${#profiles[@]} -gt 0 ]]; then
        for i in "${!profiles[@]}"; do
            if [[ "${profiles[i]}" == "$current" ]]; then
                next_index=$(( (i + 1) % ${#profiles[@]} ))
                next="${profiles[next_index]}"
                break
            fi
        done
    fi
    $PPCTL set "$next" >/dev/null 2>&1 || exit 0
    current="$next"
fi

icon=""
case "$current" in
    performance) icon="" ;;
    balanced) icon="" ;;
    power-saver) icon="" ;;
esac

tooltip="Power profile: $current\\nClick to cycle"
printf '{"text":"%s","tooltip":"%s"}\n' "$icon" "$tooltip"
