desktop=$(bspc query --desktop $1:focused --desktops --names)
layout=$(bsp-layout get $desktop)
echo $layout


