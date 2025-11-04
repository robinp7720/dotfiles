#!/usr/bin/env bash

set -uo pipefail

helper="${AURHELPER:-paru}"

case "$helper" in
  paru)
    cmd=(paru -Syu --noconfirm --sudoloop --devel --pgpfetch)
    ;;
  yay)
    cmd=(yay -Syu --noconfirm --sudoloop --devel --timeupdate --batchinstall=false --pgpfetch)
    ;;
  *)
    dunstify -i preferences-system-notifications-symbolic "Updates skipped" "Unsupported AUR helper: ${helper}"
    exit 1
    ;;
esac

if ! command -v "${cmd[0]}" >/dev/null 2>&1; then
  dunstify -i preferences-system-notifications-symbolic "Updates failed" "AUR helper ${cmd[0]} not found"
  exit 1
fi

"${cmd[@]}"
status=$?

# Send notification if update was successful
if [ $status -eq 0 ]; then
    dunstify -i preferences-system-notifications-symbolic "Updates installed" "All updates have been installed"
else
    dunstify -i preferences-system-notifications-symbolic "Updates failed" "There was an error while installing updates"
fi

exit $status
