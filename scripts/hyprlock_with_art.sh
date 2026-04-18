#!/usr/bin/env bash
# Compatibility wrapper for the shared session lock helper.

set -euo pipefail

exec "$(dirname "$0")/session_lock.sh" "$@"
