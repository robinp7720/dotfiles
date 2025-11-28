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
  "eww"
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

# Link user systemd units
mkdir -p "$HOME/.config/systemd/user"
for unit in "$DIR"/systemd/user/*.service; do
  [[ -e "$unit" ]] || continue
  target="$HOME/.config/systemd/user/$(basename "$unit")"
  if [[ -L "$target" ]]; then
    rm -f -- "$target"
  elif [[ -e "$target" ]]; then
    backup="${target}.bak.$(date +%s)"
    echo "Backing up existing $target to $backup"
    mv -- "$target" "$backup"
  fi
  ln -sfn "$unit" "$target"
done

if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
  systemctl --user enable --now auto-spotify.service || true
fi
