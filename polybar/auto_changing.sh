
bspc subscribe node_focus | 
    while read -a msg ; do
        desk_id=${msg[2]}
        wid=${msg[3]}
        title=$(xtitle $wid)
    
        class=$(xprop -id $wid WM_CLASS)



        export BACKGROUND_COLOR="#222222"

        if [[ $title =~ .*vim.*  ]]; then
            export BACKGROUND_COLOR="#444444"
        fi
        
        if [[ $class =~ .*Termite.*  ]]; then
            export BACKGROUND_COLOR="#0D1926"
        fi
        
        if [[ $LAST_COLOR != $BACKGROUND_COLOR ]]; then
            killall polybar

            polybar main &
            polybar secondary &
            polybar third &
        fi

        LAST_COLOR=$BACKGROUND_COLOR
    done



