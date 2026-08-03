#!/usr/bin/env bash
# Render the privacy-bounded Vigil snapshot for Hyprlock labels.

set -euo pipefail

mode="${1:-summary}"
VIGIL_BIN="${VIGIL_BIN-$(command -v vigil || true)}"

case "$mode" in
  summary|timer) ;;
  *)
    printf 'Usage: %s {summary|timer}\n' "$0" >&2
    exit 2
    ;;
esac

[[ -n "$VIGIL_BIN" ]] || exit 0
snapshot="$($VIGIL_BIN lockscreen snapshot 2>/dev/null || true)"
[[ -n "$snapshot" ]] || exit 0

LOCKSCREEN_SNAPSHOT="$snapshot" python3 - "$mode" <<'PY'
import html
import json
import os
import sys

try:
    response = json.loads(os.environ["LOCKSCREEN_SNAPSHOT"])
    snapshot = response["snapshot"]
except (KeyError, TypeError, json.JSONDecodeError):
    raise SystemExit(0)

mode = sys.argv[1]
if mode == "timer":
    label = snapshot.get("active_timer_label")
    seconds = snapshot.get("active_timer_remaining_seconds")
    if isinstance(label, str) and isinstance(seconds, int):
        minutes, seconds = divmod(max(seconds, 0), 60)
        print(f"Focus · {html.escape(label)} · {minutes}:{seconds:02d}")
    raise SystemExit(0)

parts = []
network = snapshot.get("network_label")
power = snapshot.get("power_label")
unread = snapshot.get("unread_notifications")
if isinstance(network, str) and network:
    parts.append(html.escape(network))
if isinstance(power, str) and power:
    parts.append(html.escape(power))
if isinstance(unread, int):
    parts.append("No unread notifications" if unread == 0 else f"{unread} unread")
print("  ·  ".join(parts))
PY
