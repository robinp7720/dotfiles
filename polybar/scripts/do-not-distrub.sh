#!/bin/sh

notifications_enabled_icon="/usr/share/icons/Adwaita/64x64/apps/preferences-system-notifications-symbolic.symbolic.png"

active_color="#FF3333"


trap "toggle" USR1

update() {
	if [[ "$(dunstctl is-paused)" == "false" ]]; then
		echo ""
	else
		echo "%{F$active_color}ﮗ"
	fi
}


toggle() {
	if [[ "$(dunstctl is-paused)" == "true" ]]; then
		echo ""
		dunstctl set-paused toggle
		dunstify -i "$notifications_enabled_icon" "Notifications" "Enabled"
	else
		echo "%{F$active_color}ﮗ"
		dunstify -i "$notifications_enabled_icon" "Notifications" "Disabled"
		sleep 1
		dunstctl close
		dunstctl set-paused toggle
	fi	
}

while true; do
	update
    sleep 600 &
    sleep_pid=$!
	wait
done
