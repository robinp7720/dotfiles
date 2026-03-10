#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../helpers/mode_common.sh"

load_environment
"$SCRIPT_DIR/external_only.sh"
bspc desktop -l monocle

if command -v steam >/dev/null 2>&1; then
  steam -start steam://open/bigpicture -fulldesktopres
fi
