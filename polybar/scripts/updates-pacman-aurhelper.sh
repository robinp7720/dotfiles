#!/usr/bin/env bash

set -euo pipefail

helper="${AURHELPER:-paru}"

case "$helper" in
  paru)
    aur_cmd=(paru -Qum --devel)
    ;;
  yay)
    aur_cmd=(yay -Qum --devel --timeupdate)
    ;;
  *)
    aur_cmd=()
    ;;
esac

pacmanupdates=""
updates_arch=0
if command -v checkupdates >/dev/null 2>&1; then
  pacmanupdates="$(checkupdates 2>/dev/null || true)"
  if [[ -n "$pacmanupdates" ]]; then
    updates_arch=$(printf '%s\n' "$pacmanupdates" | sed '/^\s*$/d' | wc -l)
  fi
fi

aurupdates=""
updates_aur=0
if (( ${#aur_cmd[@]} > 0 )) && command -v "${aur_cmd[0]}" >/dev/null 2>&1; then
  aurupdates="$("${aur_cmd[@]}" 2>/dev/null || true)"
  if [[ -n "$aurupdates" ]]; then
    updates_aur=$(printf '%s\n' "$aurupdates" | sed '/^\s*$/d' | wc -l)
  fi
fi

updates=$((updates_arch + updates_aur))

updatetext=""
if (( updates_arch > 0 )); then
  updatetext="$pacmanupdates"
fi
if (( updates_aur > 0 )); then
  if [[ -n "$updatetext" ]]; then
    updatetext+=$'\n'
  fi
  updatetext+="$aurupdates"
fi

if (( updates > 0 )) && [[ -n "$updatetext" ]]; then
  ~/.dotfiles/scripts/update-notification.sh "$updatetext" &
fi

printf '%s\n' "$updates"
