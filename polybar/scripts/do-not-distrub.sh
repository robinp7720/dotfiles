#!/bin/sh

trap "toggle" USR1

update() {
	if [[ "$(dunstctl is-paused)" == "true" ]]; then
		echo "ﮗ"
	else
		echo ""
	fi
}


toggle() {
	dunstctl set-paused toggle

	if [[ "$(dunstctl is-paused)" == "false" ]]; then
		dunstify "Notifications enabled"
	fi

	update
}

while true; do
	update
    sleep 600 &
    sleep_pid=$!
	wait
done
