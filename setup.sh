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

remove_managed_symlink() {
  local target="$1"
  local expected="$2"

  if symlink_points_to "$target" "$expected"; then
    rm -f -- "$target"
  fi
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

# Complete every build before changing installed configuration or services.
for tool in bar launcher; do
  if [[ ! -f "$DIR/tools/$tool/Cargo.toml" ]]; then
    git -C "$DIR" submodule update --init --recursive -- "tools/$tool"
  fi
done

for tool in bar launcher tv-mode greeter wallpaper-adapter; do
  echo "Building $tool (release)"
  cargo build --release --locked --bins \
    --manifest-path "$DIR/tools/$tool/Cargo.toml" \
    --target-dir "$DIR/tools/$tool/target"
done

"$DIR/scripts/build_greeter_handoff.sh"
echo "Building Hyprland swipe-event plugin"
make -C "$DIR/tools/hypr-swipe-events" all

# Create symlinks for dotfiles
link_path "$DIR/zshrc" "$HOME/.zshrc"

mkdir -p "$HOME/.config"

directories=(
  "hypr"
  "kitty"
  "matugen"
  "niri"
  "nvim"
  "shell-greeter"
  "greetd"
  "tv-mode"
)

for dir in "${directories[@]}"; do
  if [[ ! -d "$DIR/$dir" ]]; then
    warn "skipping missing configuration source: $DIR/$dir"
    continue
  fi
  echo "Linking $dir configuration"
  target="$HOME/.config/$dir"
  link_path "$DIR/$dir" "$target"
done

# Remove links created by older versions without touching independently
# managed configuration at the same paths.
legacy_directories=(
  "anyrun"
  "bspwm"
  "cairo-dock"
  "dunst"
  "eww"
  "polybar"
  "sxhkd"
  "waybar"
)
for dir in "${legacy_directories[@]}"; do
  remove_managed_symlink "$HOME/.config/$dir" "$DIR/$dir"
done
# Preserve Luma preferences; add the desktop wallpaper adapter only if unconfigured.
mkdir -p "$HOME/.config/Luma"
link_path "$DIR/Luma/style.css" "$HOME/.config/Luma/style.css"
"$DIR/scripts/configure_luma_wallpapers.sh"

echo "Linking Vigil configuration"
link_path "$DIR/bar" "$HOME/.config/vigil"
remove_managed_symlink "$HOME/.config/cockpit-bar" "$DIR/bar"
remove_managed_symlink "$HOME/.config/rofi" "$DIR/rofi"

# Setup custom scripts in ~/.local/bin
echo "Setting up scripts in ~/.local/bin"
mkdir -p "$HOME/.local/bin"

scripts=(
  "scripts/launch_kitty.sh:kitty"
  "scripts/preview.sh:preview"
  "tools/wallpaper-adapter/target/release/luma-wallpaper-dotfiles:luma-wallpaper-dotfiles"
)

for script_pair in "${scripts[@]}"; do
  src_rel="${script_pair%%:*}"
  name="${script_pair##*:}"
  
  src="$DIR/$src_rel"
  target="$HOME/.local/bin/$name"

  link_path "$src" "$target"
  echo "Linked $target -> $src"
done

if [[ -x "$DIR/tools/launcher/target/release/Luma" ]]; then
  link_path "$DIR/tools/launcher/target/release/Luma" "$HOME/.local/bin/Luma"
  echo "Linked $HOME/.local/bin/Luma -> $DIR/tools/launcher/target/release/Luma"
else
  remove_managed_symlink "$HOME/.local/bin/Luma" "$DIR/tools/launcher/target/release/Luma"
  warn "skipping Luma binary link because tools/launcher/target/release/Luma is missing"
fi

if [[ -x "$DIR/tools/launcher/target/release/luma-mail-eds" ]]; then
  link_path "$DIR/tools/launcher/target/release/luma-mail-eds" "$HOME/.local/bin/luma-mail-eds"
  echo "Linked $HOME/.local/bin/luma-mail-eds -> $DIR/tools/launcher/target/release/luma-mail-eds"
else
  remove_managed_symlink "$HOME/.local/bin/luma-mail-eds" "$DIR/tools/launcher/target/release/luma-mail-eds"
  warn "skipping optional Evolution helper link because tools/launcher/target/release/luma-mail-eds is missing"
fi

if [[ -x "$DIR/tools/bar/target/release/vigil" ]]; then
  link_path "$DIR/tools/bar/target/release/vigil" "$HOME/.local/bin/vigil"
  echo "Linked $HOME/.local/bin/vigil -> $DIR/tools/bar/target/release/vigil"
else
  remove_managed_symlink "$HOME/.local/bin/vigil" "$DIR/tools/bar/target/release/vigil"
  warn "skipping Vigil binary link because tools/bar/target/release/vigil is missing"
fi
remove_managed_symlink "$HOME/.local/bin/cockpit-bar" "$DIR/tools/bar/target/release/cockpit-bar"

if [[ -x "$DIR/tools/tv-mode/target/release/tv-mode" ]]; then
  link_path "$DIR/tools/tv-mode/target/release/tv-mode" "$HOME/.local/bin/tv-mode"
  echo "Linked $HOME/.local/bin/tv-mode -> $DIR/tools/tv-mode/target/release/tv-mode"
else
  remove_managed_symlink "$HOME/.local/bin/tv-mode" "$DIR/tools/tv-mode/target/release/tv-mode"
  warn "skipping tv-mode binary link because tools/tv-mode/target/release/tv-mode is missing"
fi

remove_managed_symlink "$HOME/.local/bin/rofi" "$DIR/scripts/rofi_wrapper.sh"

make -C "$DIR/tools/hypr-swipe-events" install \
  "PLUGIN_DIR=${XDG_DATA_HOME:-$HOME/.local/share}/hyprland/plugins"

# Link user systemd units
mkdir -p "$HOME/.config/systemd/user"
for unit in "$DIR"/systemd/user/*.service "$DIR"/systemd/user/*.timer; do
  [[ -e "$unit" ]] || continue
  target="$HOME/.config/systemd/user/$(basename "$unit")"
  link_path "$unit" "$target"
done
remove_managed_symlink \
  "$HOME/.config/systemd/user/cockpit-bar.service" \
  "$DIR/systemd/user/cockpit-bar.service"

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

# Install the standalone login surface only when system installation is enabled.
if [[ "${INSTALL_SYSTEM_CONFIG:-1}" == "1" ]] && command -v sudo >/dev/null 2>&1; then
  sudo bash "$DIR/scripts/install_greeter.sh" "$USER"
  if ! "$DIR/scripts/awww_wallpaper_watcher.sh" --greeter-only; then
    warn "greeter installed with fallback assets; wallpaper publication failed"
  fi
else
  echo "Skipping system greeter installation (disabled or sudo unavailable)"
fi
