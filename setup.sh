#!/usr/bin/env bash

set -euo pipefail

# Get the directory of the script
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "Setting up dotfiles from $DIR"

# Create symlinks for dotfiles
ln -sf "$DIR/zshrc" "$HOME/.zshrc"

mkdir -p "$HOME/.config"

directories=(
  "bspwm"
  "dunst"
  "hypr"
  "kitty"
  "matugen"
  "niri"
  "polybar"
  "rofi"
  "sxhkd"
  "termite"
  "waybar"
)

for dir in "${directories[@]}"; do
  echo "Linking $dir configuration"
  target="$HOME/.config/$dir"
  if [[ -L "$target" ]]; then
    rm -f -- "$target"
  elif [[ -e "$target" ]]; then
    backup="${target}.bak.$(date +%s)"
    echo "Backing up existing $target to $backup"
    mv -- "$target" "$backup"
  fi
  ln -sfn "$DIR/$dir" "$target"
done
