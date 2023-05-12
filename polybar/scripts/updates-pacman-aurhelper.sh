#!/bin/sh

pacmanupdates=$(checkupdates)
aurupdates=$(yay -Qum --devel --timeupdate)

updates_arch=$(checkupdates | wc -l)
updates_aur=$(yay -Qum --devel --timeupdate | wc -l)

updates=$(("$updates_arch" + "$updates_aur"))

updatetext="$pacmanupdates\n$aurupdates"

if [ "$updates_arch" -eq 0 ]; then
    updatetext="$aurupdates"
fi

# If more then 0 updates
if [ "$updates" -gt 0 ]; then
    ~/.dotfiles/scripts/update-notification.sh "$updatetext" &
fi

echo $updates
