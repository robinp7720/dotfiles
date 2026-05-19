#!/bin/sh

lock_file="${XDG_RUNTIME_DIR:-/tmp}/center-tws-login.lock"
exec 9>"$lock_file" || exit 0
flock -n 9 || exit 0

center_tws_login() {
    workspace="$(hyprctl activeworkspace -j 2>/dev/null | jq -r '.name // empty')"
    [ -n "$workspace" ] || return 0

    min_x="$(hyprctl monitors -j 2>/dev/null | jq 'map(.x) | min')"
    min_y="$(hyprctl monitors -j 2>/dev/null | jq 'map(.y) | min')"
    max_x="$(hyprctl monitors -j 2>/dev/null | jq 'map(.x + .width) | max')"
    max_y="$(hyprctl monitors -j 2>/dev/null | jq 'map(.y + .height) | max')"
    [ -n "$min_x" ] && [ -n "$min_y" ] && [ -n "$max_x" ] && [ -n "$max_y" ] || return 0

    hyprctl clients -j 2>/dev/null |
        jq -r \
            --argjson min_x "$min_x" \
            --argjson min_y "$min_y" \
            --argjson max_x "$max_x" \
            --argjson max_y "$max_y" \
            '.[] |
             select(.class == "install4j-jclient-Launcher" and .title == "Login") |
             select((.at[0] < $min_x) or (.at[1] < $min_y) or ((.at[0] + .size[0]) > $max_x) or ((.at[1] + .size[1]) > $max_y)) |
             .address' |
        while IFS= read -r address; do
            [ -n "$address" ] || continue
            hyprctl dispatch movetoworkspacesilent "$workspace,address:$address" >/dev/null
            hyprctl dispatch focuswindow "address:$address" >/dev/null
            hyprctl dispatch centerwindow >/dev/null
        done
}

while :; do
    center_tws_login
    sleep 1
done
