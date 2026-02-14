# Robin's Dotfiles

Personal configuration files that power both my Wayland (Hyprland) and X11 (bspwm) desktops. The repo bundles window manager layouts, status bars, launchers, terminal themes, and helper scripts so the environment can be reproduced quickly on a fresh Arch-based install.

## Highlights
- **Hyprland setup** with shared configuration, multi-monitor layouts (`hypr/monitor_layouts`), and scripts for switching between default, game, and external display modes. Cursor theme is enforced via env vars to avoid the Hypr logo pointer.
- **BSPWM workflow** including focus automation, per-mode screenlayout scripts, and integration with `sxhkd`, `polybar`, and `nitrogen`.
- **Dynamic theming via Matugen** that renders matched color palettes for Hyprland, Waybar, Kitty (with an automatic USR1 reload hook), Rofi, Dunst, and greetd/nwg-hello from a single template directory.
- **Productivity bars**: Polybar modules cover GPU-friendly clipboard-to-GPT prompts, pacman/AUR update counters with notifications, bluetooth, MPD, and more; Waybar styles live alongside Wayland configs and include a clickable power menu.
- **Shell environment** built on Oh-My-Zsh with curated aliases (eza, bat, dust, devour, etc.), `fortune` greeting, and helper functions for toolchains.

## Requirements
These dotfiles assume an Arch Linux (or derivative) system with the following core packages available:

- Window managers and daemons: `hyprland`, `hypridle`, `hyprpaper`, `hyprsunset`, `bspwm`, `sxhkd`, `nitrogen`
- Greeter: `greetd`, `nwg-hello`
- Bars and launchers: `polybar`, `waybar`, `rofi`, `dunst`, `rofi-wayland` (or equivalent), `cairo-dock`
- Terminals and utilities: `kitty`, `picom`, `redshift`, `thunar`, `mpd-notification`, `xclip`, `dunstify`
- Theming and helpers: `matugen`, `sgpt` (placed at `~/.local/bin/sgpt` for the GPT clipboard module), `checkupdates` (from `pacman-contrib`)
- AUR helper configured through the `AURHELPER` variable (`paru` by default, `yay` supported); both are used by Polybar and `scripts/update.sh`

Adjust the list as needed for your distro (some scripts expect Wayland- or X11-specific binaries, or NVIDIA monitor names such as `DP-5` / `DVI-D-2`).

## Getting Started
1. **Clone the repo**
   ```bash
   git clone https://github.com/robinp7720/dotfiles.git ~/.dotfiles
   cd ~/.dotfiles
   ```
2. **Back up existing config** – the setup script removes `~/.config/<dir>` before recreating the symlink. Make a copy if you have existing settings you care about.
3. **Run the linker**
   ```bash
   chmod +x setup.sh
   ./setup.sh
   ```
   The script will link `zshrc` to `~/.zshrc` and replace the directories listed in `setup.sh` inside `~/.config/`. If `sudo` is available and `/etc/{nwg-hello,greetd}` exist, it will also install/link the greetd + nwg-hello configs and point their CSS to Matugen’s cache.
4. **Restart your session** or reload individual applications (e.g. `polybar`, `waybar`, `hyprland reload`) to pick up the new configuration.

## Repo Layout
- `hypr/` – Hyprland configuration split into `hyprland-config/`, monitor layouts, colors, idle/lock settings, and helper scripts.
- `bspwm/` – Window manager config with per-mode scripts (`modes/`), helpers, and the main `bspwmrc`.
- `polybar/` – Shared colors, modules, and scripts (including the clipboard-to-GPT helper and update notifications).
- `waybar/` – Waybar configuration and CSS theme generated via Matugen, with a custom power button wired to `scripts/power_menu.sh`.
- `matugen/` – Template files and `config.toml` describing how colors propagate across Hyprland, Waybar, Kitty (auto-reloads via USR1), Dunst, and greetd/nwg-hello.
- `rofi/`, `kitty/`, `dunst/` – Application-specific themes.
- `scripts/` – Utility scripts such as `update.sh` (runs `paru`/`yay` with notifications), `update-notification.sh` (launches an update terminal via Dunst actions), and `power_menu.sh` (logout/shutdown/reboot via Waybar/Rofi).
- `arch-auto-update/` – Template config for unattended background updates (`~/.config/arch-auto-update/config.env`).
- `greetd/`, `nwg-hello/` – Login screen configuration; Matugen generates `greetd.css` into `/var/cache/matugen/` and setup links it into `/etc/nwg-hello`.
- `zshrc` – Oh-My-Zsh setup, aliases, toolchain initializers, and environment sourcing.

## Customization Notes
- **Monitor naming**: Update `hypr/hyprland-config/desktop.conf` and the files under `hypr/monitor_layouts/` or `bspwm/modes/` if your outputs differ (`DP-5`, `DVI-D-2`, `HDMI-0`, etc.).
- **Hyprland modes**: Run the scripts from `hypr/scripts/modes/` to swap monitor layouts (`default.sh`, `game.sh`, `game_external.sh`). Each overwrites `~/.config/hypr/monitors.conf`.
- **BSPWM modes**: Trigger the scripts in `bspwm/modes/` (e.g. `all_monitors.sh`, `external_only.sh`) to adjust workspace assignments, padding, and Polybar/Nitrogen behavior.
- **Theme generation**: After installing Matugen, run it to regenerate color schemes across Waybar, Hyprland, Kitty, Dunst, and greetd/nwg-hello using the templates in `matugen/templates/`. Kitty reloads automatically via a USR1 signal once its colors file is written.
- **Greetd/NWG-Hello**: `setup.sh` links `greetd/config.toml` and `nwg-hello/hyprland.conf` into `/etc`, and symlinks `/etc/nwg-hello/nwg-hello.css` to the Matugen-generated cache at `/var/cache/matugen/greetd.css`. Backups of existing files are created with timestamps.
- **Shell tweaks**: Edit `AURHELPER` and plugin lists directly in `zshrc`; aliases assume tools such as `eza`, `bat`, `dust`, and `devour` are installed.
- **Polybar GPT module**: Requires `sgpt` and `xclip`. Send `USR1` to the module’s process (e.g. clicking the Polybar module) to transform clipboard prompts and return generated text.

## Updating Packages
Use the Polybar update module (`updates-pacman-aurhelper`) or run the helper manually:
```bash
AURHELPER=paru ~/.dotfiles/scripts/update.sh
```
Successful runs fire a Dunst notification; failures notify as well. Set `AURHELPER` to `yay` if that’s your preferred helper.

### Automatic background updates
This repo also provides user-level `systemd` automation:

- Service: `arch-auto-update.service`
- Timer: `arch-auto-update.timer` (daily at `03:00` with up to `2h` randomized delay)
- Config: `~/.config/arch-auto-update/config.env`
- Logs/state: `~/.local/state/arch-auto-update/`

The updater:
- Uses the configured `AURHELPER` (`paru` or `yay`)
- Runs non-interactive `-Syu`
- Stores full logs and JSON summaries
- Sends Dunst notifications for success, failures, manual intervention hints, package notices, reboot-likely updates, and `.pacnew/.pacsave` files
- Writes notification diagnostics to process stderr (visible in user journal)

Requirement:
- Non-interactive `sudo` for pacman must be available (`sudo -n /usr/bin/pacman -V`), otherwise the run is marked as manual-intervention-required and no updates are installed.
- Helper script to configure this:
  ```bash
  sudo ~/.dotfiles/scripts/setup-arch-auto-update-sudo.sh --user "$USER"
  ```
  To remove the rule later:
  ```bash
  sudo ~/.dotfiles/scripts/setup-arch-auto-update-sudo.sh --remove --user "$USER"
  ```

Recommended background config (to avoid long "stuck" AUR rebuilds):
- `AUTO_UPDATE_DEVEL=0` keeps VCS/devel packages out of automatic refresh.
- `UPDATE_TIMEOUT=4h` limits maximum run time.
- Both are configurable in `~/.config/arch-auto-update/config.env`.

Manual controls:
```bash
systemctl --user status arch-auto-update.timer
systemctl --user start arch-auto-update.service
journalctl --user -u arch-auto-update.service -n 100 --no-pager
```

## License
No explicit license has been provided. Treat the contents as personal reference unless you have permission to reuse them.
