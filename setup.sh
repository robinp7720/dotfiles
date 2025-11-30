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
  "nwg-hello"
  "greetd"
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

# Configure greetd/nwg-hello CSS to point at matugen output (requires sudo)
if command -v sudo >/dev/null 2>&1 && [[ -d /etc/nwg-hello ]]; then
  cache_dir="/var/cache/matugen"
  cache_css="${cache_dir}/greetd.css"
  target_css="/etc/nwg-hello/nwg-hello.css"
  target_hypr="/etc/nwg-hello/hyprland.conf"
  target_monitors="/etc/nwg-hello/monitors.conf"

  sudo mkdir -p "$cache_dir"
  sudo chown "$USER":"$USER" "$cache_dir"

  if [[ ! -f "$cache_css" ]]; then
    sudo cp "$DIR/matugen/templates/greetd.css" "$cache_css"
    sudo chown "$USER":"$USER" "$cache_css"
  fi

  # Install Hyprland config for greetd/nwg-hello
  if [[ -e "$DIR/nwg-hello/hyprland.conf" ]]; then
    if [[ -L "$target_hypr" || -e "$target_hypr" ]]; then
      backup="${target_hypr}.bak.$(date +%s)"
      echo "Backing up existing $target_hypr to $backup (requires sudo)"
      sudo mv -- "$target_hypr" "$backup"
    fi
    sudo install -m 644 "$DIR/nwg-hello/hyprland.conf" "$target_hypr"
    echo "Installed greetd Hyprland config to $target_hypr"
  fi

  # Install monitor layout for greetd Hyprland (copied from main Hypr config)
  if [[ -e "$DIR/hypr/monitors.conf" ]]; then
    if [[ -L "$target_monitors" || -e "$target_monitors" ]]; then
      backup="${target_monitors}.bak.$(date +%s)"
      echo "Backing up existing $target_monitors to $backup (requires sudo)"
      sudo mv -- "$target_monitors" "$backup"
    fi
    sudo install -m 644 "$DIR/hypr/monitors.conf" "$target_monitors"
    echo "Installed greetd monitor config to $target_monitors"
  fi

  if [[ ! -L "$target_css" && -e "$target_css" ]]; then
    backup="${target_css}.bak.$(date +%s)"
    echo "Backing up existing $target_css to $backup (requires sudo)"
    sudo mv -- "$target_css" "$backup"
  fi

  sudo ln -sfn "$cache_css" "$target_css"
  echo "Linked greetd CSS: $target_css -> $cache_css"
else
  echo "Skipping greetd CSS link (sudo unavailable or /etc/nwg-hello missing)"
fi

# Install greetd daemon config
if command -v sudo >/dev/null 2>&1 && [[ -d /etc/greetd ]]; then
  target_greetd_cfg="/etc/greetd/config.toml"
  if [[ ! -L "$target_greetd_cfg" && -e "$target_greetd_cfg" ]]; then
    backup="${target_greetd_cfg}.bak.$(date +%s)"
    echo "Backing up existing $target_greetd_cfg to $backup (requires sudo)"
    sudo mv -- "$target_greetd_cfg" "$backup"
  fi
  sudo ln -sfn "$DIR/greetd/config.toml" "$target_greetd_cfg"
  echo "Linked greetd config: $target_greetd_cfg -> $DIR/greetd/config.toml"
else
  echo "Skipping greetd config (sudo unavailable or /etc/greetd missing)"
fi
