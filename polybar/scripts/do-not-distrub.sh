#!/bin/sh

notifications_enabled_icon="/usr/share/icons/Adwaita/64x64/apps/preferences-system-notifications-symbolic.symbolic.png"

trap "toggle" USR1

update() {
	if [[ "$(dunstctl is-paused)" == "true" ]]; then
		echo "%{F#FF3333}ﮗ"
	else
		echo ""
	fi
}


toggle() {
	dunstctl set-paused toggle

	if [[ "$(dunstctl is-paused)" == "false" ]]; then
		dunstify -i "$notifications_enabled_icon" "Notifications" "Enabled"
	fi

	update
}

while true; do
	update
    sleep 600 &
    sleep_pid=$!
	wait
done
