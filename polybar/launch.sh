#!/usr/bin/env bash

# Terminate already running bar instances
polybar-msg cmd quit

# Launch "main" bar
echo "---" | tee -a /tmp/polybar_main.log
polybar main -c ~/.dotfiles/polybar/config.ini 2>&1 | tee -a /tmp/polybar_main.log & disown

echo "Polybar launched..."
