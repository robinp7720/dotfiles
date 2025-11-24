#!/usr/bin/env bash
# Refresh album art, then launch hyprlock.

set -euo pipefail

"$(dirname "$0")/update_album_art.sh"
exec hyprlock "$@"
