#!/usr/bin/env bash
# greetd starts the authenticated session only after this compositor exits.
set -euo pipefail

/usr/local/bin/shell-greeter --config /etc/shell-greeter/config.toml || true
hyprctl dispatch 'hl.dsp.exit()'
