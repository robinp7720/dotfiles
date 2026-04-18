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

  if ! sudo test -L "$target" && sudo test -e "$target" && sudo cmp -s "$src" "$target"; then
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
if command -v sudo >/dev/null 2>&1; then
  if sudo install -d -m 755 /etc/nwg-hello; then
    cache_dir="/var/cache/matugen"
    cache_css="${cache_dir}/greetd.css"
    target_css="/etc/nwg-hello/nwg-hello.css"
    target_hypr="/etc/nwg-hello/hyprland.conf"
    target_base="/etc/nwg-hello/base.conf"
    target_monitors="/etc/nwg-hello/monitors.conf"
    source_hypr="$DIR/nwg-hello/hyprland.conf"
    source_base="$DIR/hypr/hyprland-config/base.conf"
    source_monitors="$DIR/hypr/monitors.conf"

    if [[ ! -e "$source_monitors" ]]; then
      source_monitors="$DIR/hypr/monitor_layouts/default.conf"
    fi

    sudo mkdir -p "$cache_dir"
    sudo chown "$USER":"$USER" "$cache_dir"

    if [[ ! -f "$cache_css" ]]; then
      sudo cp "$DIR/matugen/templates/greetd.css" "$cache_css"
      sudo chown "$USER":"$USER" "$cache_css"
    fi

    # Install Hyprland config for greetd/nwg-hello as real files so the
    # greeter does not depend on being able to traverse /home.
    if [[ -e "$source_hypr" ]]; then
      sudo_install_file "$source_hypr" "$target_hypr"
      echo "Installed greetd Hyprland config to $target_hypr"
    fi

    # Install shared Hyprland base for greetd/nwg-hello as a real file.
    if [[ -e "$source_base" ]]; then
      sudo_install_file "$source_base" "$target_base"
      echo "Installed greetd shared Hyprland base to $target_base"
    fi

    # Keep greetd aligned with the current Hypr monitor layout when present,
    # otherwise fall back to the tracked default layout. Install a real file
    # for the same reason as the other greeter-side config.
    if [[ -e "$source_monitors" ]]; then
      sudo_install_file "$source_monitors" "$target_monitors"
      echo "Installed greetd monitor config to $target_monitors"
    fi

    sudo_link_path "$cache_css" "$target_css"
    echo "Linked greetd CSS: $target_css -> $cache_css"
  else
    warn "failed to create /etc/nwg-hello; skipping greetd CSS and Hyprland config"
  fi
else
  echo "Skipping greetd CSS link (sudo unavailable)"
fi

# Install greetd daemon config
if command -v sudo >/dev/null 2>&1; then
  if sudo install -d -m 755 /etc/greetd; then
    target_greetd_cfg="/etc/greetd/config.toml"
    sudo_install_file "$DIR/greetd/config.toml" "$target_greetd_cfg"
    echo "Installed greetd config to $target_greetd_cfg"
  else
    warn "failed to create /etc/greetd; skipping greetd config"
  fi
else
  echo "Skipping greetd config (sudo unavailable)"
fi
