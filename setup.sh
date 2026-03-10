#!/usr/bin/env bash

set -euo pipefail

# Get the directory of the script
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

warn() {
  printf 'Warning: %s\n' "$*" >&2
}

backup_existing() {
  local target="$1"

  if [[ -L "$target" ]]; then
    rm -f -- "$target"
  elif [[ -e "$target" ]]; then
    local backup="${target}.bak.$(date +%s)"
    echo "Backing up existing $target to $backup"
    mv -- "$target" "$backup"
  fi
}

sudo_backup_existing() {
  local target="$1"

  if sudo test -L "$target"; then
    sudo rm -f -- "$target"
  elif sudo test -e "$target"; then
    local backup="${target}.bak.$(date +%s)"
    echo "Backing up existing $target to $backup (requires sudo)"
    sudo mv -- "$target" "$backup"
  fi
}

symlink_points_to() {
  local target="$1"
  local expected="$2"

  [[ -L "$target" ]] && [[ "$(readlink "$target")" == "$expected" ]]
}

sudo_symlink_points_to() {
  local target="$1"
  local expected="$2"
  local current

  current="$(sudo readlink "$target" 2>/dev/null || true)"
  [[ -n "$current" ]] && [[ "$current" == "$expected" ]]
}

link_path() {
  local src="$1"
  local target="$2"

  if symlink_points_to "$target" "$src"; then
    return 0
  fi

  backup_existing "$target"
  ln -sfn "$src" "$target"
}

sudo_link_path() {
  local src="$1"
  local target="$2"

  if sudo_symlink_points_to "$target" "$src"; then
    return 0
  fi

  sudo_backup_existing "$target"
  sudo ln -sfn "$src" "$target"
}

sudo_install_file() {
  local src="$1"
  local target="$2"

  if sudo test -e "$target" && sudo cmp -s "$src" "$target"; then
    return 0
  fi

  sudo_backup_existing "$target"
  sudo install -m 644 "$src" "$target"
}

echo "Setting up dotfiles from $DIR"

# Create symlinks for dotfiles
link_path "$DIR/zshrc" "$HOME/.zshrc"

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
  "waybar"
  "nwg-hello"
  "greetd"
  "anyrun"
)

for dir in "${directories[@]}"; do
  echo "Linking $dir configuration"
  target="$HOME/.config/$dir"
  link_path "$DIR/$dir" "$target"
done

# Setup custom scripts in ~/.local/bin
echo "Setting up scripts in ~/.local/bin"
mkdir -p "$HOME/.local/bin"

scripts=(
  "scripts/launch_kitty.sh:kitty"
  "scripts/rofi_wrapper.sh:rofi"
)

for script_pair in "${scripts[@]}"; do
  src_rel="${script_pair%%:*}"
  name="${script_pair##*:}"
  
  src="$DIR/$src_rel"
  target="$HOME/.local/bin/$name"

  link_path "$src" "$target"
  echo "Linked $target -> $src"
done

# Link user systemd units
mkdir -p "$HOME/.config/systemd/user"
for unit in "$DIR"/systemd/user/*.service "$DIR"/systemd/user/*.timer; do
  [[ -e "$unit" ]] || continue
  target="$HOME/.config/systemd/user/$(basename "$unit")"
  link_path "$unit" "$target"
done

if command -v systemctl >/dev/null 2>&1; then
  if ! systemctl --user daemon-reload; then
    warn "systemctl --user daemon-reload failed"
  fi
  if [[ "${AUTO_ENABLE_SPOTIFY_SERVICE:-1}" == "1" ]]; then
    if command -v pactl >/dev/null 2>&1 && command -v spotify >/dev/null 2>&1; then
      if ! systemctl --user enable --now auto-spotify.service; then
        warn "failed to enable/start auto-spotify.service"
      fi
    else
      warn "skipping auto-spotify.service enablement because pactl or spotify is unavailable"
    fi
  fi
fi

# Configure greetd/nwg-hello CSS to point at matugen output (requires sudo)
if command -v sudo >/dev/null 2>&1 && [[ -d /etc/nwg-hello ]]; then
  cache_dir="/var/cache/matugen"
  cache_css="${cache_dir}/greetd.css"
  target_css="/etc/nwg-hello/nwg-hello.css"
  target_hypr="/etc/nwg-hello/hyprland.conf"
  target_base="/etc/nwg-hello/base.conf"
  target_monitors="/etc/nwg-hello/monitors.conf"

  sudo mkdir -p "$cache_dir"
  sudo chown "$USER":"$USER" "$cache_dir"

  if [[ ! -f "$cache_css" ]]; then
    sudo cp "$DIR/matugen/templates/greetd.css" "$cache_css"
    sudo chown "$USER":"$USER" "$cache_css"
  fi

  # Install Hyprland config for greetd/nwg-hello
  if [[ -e "$DIR/nwg-hello/hyprland.conf" ]]; then
    sudo_install_file "$DIR/nwg-hello/hyprland.conf" "$target_hypr"
    echo "Installed greetd Hyprland config to $target_hypr"
  fi

  # Install shared Hyprland base for greetd/nwg-hello
  if [[ -e "$DIR/hypr/hyprland-config/base.conf" ]]; then
    sudo_install_file "$DIR/hypr/hyprland-config/base.conf" "$target_base"
    echo "Installed greetd shared Hyprland base to $target_base"
  fi

  # Install monitor layout for greetd Hyprland (copied from main Hypr config)
  if [[ -e "$DIR/hypr/monitors.conf" ]]; then
    sudo_install_file "$DIR/hypr/monitors.conf" "$target_monitors"
    echo "Installed greetd monitor config to $target_monitors"
  fi

  sudo_link_path "$cache_css" "$target_css"
  echo "Linked greetd CSS: $target_css -> $cache_css"
else
  echo "Skipping greetd CSS link (sudo unavailable or /etc/nwg-hello missing)"
fi

# Install greetd daemon config
if command -v sudo >/dev/null 2>&1 && [[ -d /etc/greetd ]]; then
  target_greetd_cfg="/etc/greetd/config.toml"
  sudo_link_path "$DIR/greetd/config.toml" "$target_greetd_cfg"
  echo "Linked greetd config: $target_greetd_cfg -> $DIR/greetd/config.toml"
else
  echo "Skipping greetd config (sudo unavailable or /etc/greetd missing)"
fi
