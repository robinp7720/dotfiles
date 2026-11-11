#!/usr/bin/env bash
# Root-owned launcher: private backend only when its complete build ABI matches.
set -euo pipefail

handoff=/usr/local/lib/shell-greeter/handoff
unset AQ_PRESERVE_OUTPUTS_ON_EXIT
if [[ -r "$handoff/system.sha256" && -r "$handoff/libaquamarine.so.15" ]] &&
  sha256sum --check --status "$handoff/system.sha256"; then
  export LD_LIBRARY_PATH="$handoff${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export AQ_PRESERVE_OUTPUTS_ON_EXIT=1
else
  printf 'shell-greeter: private handoff backend unavailable or outdated; using stock Aquamarine.\n' >&2
fi
exec /usr/bin/start-hyprland -- --config /etc/shell-greeter/hyprland.lua
