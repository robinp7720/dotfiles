#!/bin/sh

active_color="#FF3333"

trap "generate" USR1

#update() {
#
#}


generate() {
	clipboard=$(xclip -o -selection clipboard)
	#dunstify "$clipboard"
	echo "WAIT"
	output=$($HOME/.local/bin/sgpt "$clipboard")
	#dunstify "generated" "$output"
	echo $output | xclip -selection clipboard -in
	echo "GEN"
}

echo "GEN"

while true; do
	#update
    sleep 600 &
    sleep_pid=$!
	wait
done
