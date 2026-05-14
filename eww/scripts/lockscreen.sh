#!/usr/bin/env bash
# Compatibility wrapper for older Eww lock actions.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

exec "$SCRIPT_DIR/../../scripts/session_lock.sh" "$@"
