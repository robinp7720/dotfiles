#!/usr/bin/env bash
# Render the tracked Hyprlock template for the output focused at lock time.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE="${HYPRLOCK_TEMPLATE:-$SCRIPT_DIR/../hypr/hyprlock.conf}"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
DESTINATION="${HYPRLOCK_RUNTIME_CONFIG:-$RUNTIME_DIR/hyprlock.generated.conf}"
FOCUSED_OUTPUT=""

usage() {
  printf 'Usage: %s [--output connector] [--destination path]\n' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      FOCUSED_OUTPUT="$2"
      shift 2
      ;;
    --destination)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      DESTINATION="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

json_connector() {
  python3 -c '
import json
import sys

try:
    value = json.load(sys.stdin)
except (json.JSONDecodeError, OSError):
    raise SystemExit(1)

name = None
if isinstance(value, dict):
    # Hyprland active-workspace payload has both a workspace `name` and a
    # `monitor`; the Niri focused-output payload uses `name` for the connector.
    name = value.get("monitor") or value.get("name")
if isinstance(name, str) and name:
    print(name)
' 2>/dev/null
}

detect_focused_output() {
  local output=""

  if command -v hyprctl >/dev/null 2>&1 && hyprctl monitors -j >/dev/null 2>&1; then
    output="$(hyprctl activeworkspace -j 2>/dev/null | json_connector || true)"
  fi

  if [[ -z "$output" ]] && command -v niri >/dev/null 2>&1; then
    output="$(niri msg --json focused-output 2>/dev/null | json_connector || true)"
  fi

  printf '%s\n' "$output"
}

if [[ -z "$FOCUSED_OUTPUT" ]]; then
  FOCUSED_OUTPUT="$(detect_focused_output)"
fi

# Connector names are deliberately restricted before being inserted into config.
if [[ -n "$FOCUSED_OUTPUT" && ! "$FOCUSED_OUTPUT" =~ ^[[:alnum:]_.:-]+$ ]]; then
  FOCUSED_OUTPUT=""
fi

[[ -f "$TEMPLATE" ]] || {
  printf 'Hyprlock template not found: %s\n' "$TEMPLATE" >&2
  exit 1
}

mkdir -p "$(dirname "$DESTINATION")"
temp_config="$(mktemp "$(dirname "$DESTINATION")/.hyprlock.XXXXXX")"
trap 'rm -f "$temp_config"' EXIT

sed "s|^[[:space:]]*monitor = # @dashboard-monitor@$|    monitor = $FOCUSED_OUTPUT|" \
  "$TEMPLATE" > "$temp_config"
chmod 600 "$temp_config"
mv "$temp_config" "$DESTINATION"
trap - EXIT

printf '%s\n' "$DESTINATION"
