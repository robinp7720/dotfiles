yay -Syu --noconfirm --devel --timeupdate

# Send notification if update was successful
if [ $? -eq 0 ]; then
    dunstify -i preferences-system-notifications-symbolic "Updates installed" "All updates have been installed"
else
    dunstify -i preferences-system-notifications-symbolic "Updates failed" "There was an error while installing updates"
fi
