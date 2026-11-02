#!/usr/bin/env bash
set -euo pipefail

kitty_bin="${KITTY_REAL_BIN:-/usr/bin/kitty}"

exec "$kitty_bin" "$@"
