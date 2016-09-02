if [ $1 == 'main' ]
then
	rofi -combi-modi "window,drun,ssh,run" -show combi -fuzzy
else
	rofi -show $1
fi

#rofi -show $1 -lines 3 -eh 2 -sidebar-mode -fullscreen -width 100 -padding $((($(xwininfo -root |awk '/Height/ { print $2}')/3)-100)) -opacity 83 -color-window "#2f343f" -color-normal "#2f343f, #f3f4f5, #2f343f, #333844, #dedfe0"
