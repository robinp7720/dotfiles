xbacklight -dec 5
sleep 0.01
gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications --method org.freedesktop.Notifications.Notify my_app_name 42 audio-card "Brightness" "$(xbacklight)%" [] {} 30
