#!/usr/bin/env bash

set -euo pipefail

output=$(dunstify -i preferences-system-notifications-symbolic "Updates are available" "$1" -A "default,Update")
if [[ "$output" == "default" ]]; then
    kitty --class "Update" -e ~/.dotfiles/scripts/update.sh &
fi
