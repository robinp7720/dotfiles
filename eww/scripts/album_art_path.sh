#!/usr/bin/env bash

set -euo pipefail

cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}"
art_path="${cache_dir}/hyprlock/album.jpg"
mtime="$(stat -c %Y "$art_path" 2>/dev/null || echo 0)"

printf '%s?%s\n' "$art_path" "$mtime"
