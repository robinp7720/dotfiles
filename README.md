# Robin's Dotfiles

Personal configuration files for both Wayland (Hyprland, Niri) and X11 (bspwm) desktops. The repo bundles monitor layouts, status bars, launchers, terminal themes, helper scripts, and a small Rust tool for session-aware monitor profile switching.

## Highlights
- **Hyprland setup** with shared configuration, multi-monitor layouts (`hypr/monitor_layouts`), and scripts for switching between default, game, and external display modes. Cursor theme is enforced via env vars to avoid the Hypr logo pointer.
- **BSPWM workflow** including focus automation, per-mode screenlayout scripts, and integration with `sxhkd`, `polybar`, `nitrogen`, and `superpaper`.
- **Dynamic theming via Matugen** that renders matched color palettes for Hyprland, Waybar, Kitty (with an automatic USR1 reload hook), Rofi, Dunst, and greetd/nwg-hello from a single template directory.
- **Productivity bars**: Polybar and Waybar configs live alongside the compositor configs, with custom power, Bluetooth, profile, and media helpers.
- **Control center overlay**: Eww provides a toggleable quick-actions panel for calendar context, media, Bluetooth, power profile, and session actions across Hyprland and Niri.
- **Codex self-improvement**: on login, a guarded user service can let Codex pick one small dotfiles improvement, apply it, auto-commit it, and send a desktop notification with the summary.
- **Shell environment** built on Oh-My-Zsh with curated aliases (eza, bat, dust, devour, etc.), `fortune` greeting, and helper functions for toolchains.
- **Native tools** under `tools/`: `session-manager` for hardware-aware display profiles and `launcher` for a Rust-backed app launcher source.

## Requirements
These dotfiles assume an Arch Linux (or derivative) system with the following core packages available:

- Window managers and daemons: `hyprland`, `hypridle`, `hyprpaper`, `hyprsunset`, `bspwm`, `sxhkd`, `nitrogen`
- Greeter: `greetd`, `nwg-hello`
- Bars and launchers: `polybar`, `waybar`, `rofi` or `rofi-wayland`, `dunst`, `anyrun`, `eww`, `cairo-dock`
- Terminals and utilities: `kitty`, `picom`, `redshift`, `thunar`, `mpd-notification`, `playerctl`, `xclip`, `dunstify`, `bluetoothctl`, `powerprofilesctl`, `pactl`
- Theming and helpers: `matugen`, `swww`/`awww` wallpaper tooling, `superpaper`, `sgpt` (for clipboard-to-GPT helpers)
- Optional automation: `codex` (required only for the login-time self-improvement flow)
- Optional Rust toolchain: required only if you want to rebuild binaries in `tools/`

Adjust the list as needed for your distro (some scripts expect Wayland- or X11-specific binaries, or NVIDIA monitor names such as `DP-5` / `DVI-D-2`).

## Getting Started
1. **Clone the repo**
   ```bash
   git clone https://github.com/robinp7720/dotfiles.git ~/.dotfiles
   cd ~/.dotfiles
   ```
2. **Back up existing config** – `setup.sh` replaces conflicting files or directories with timestamped backups before linking. Review anything already living under `~/.config` if you care about preserving it.
3. **Run the linker**
   ```bash
   chmod +x setup.sh
   ./setup.sh
   ```
   The script links `zshrc` to `~/.zshrc`, symlinks the configured directories into `~/.config/`, links user `systemd` units, and reloads the user daemon. If `sudo` is available and `/etc/{nwg-hello,greetd}` exist, it also installs the greetd + nwg-hello files and links CSS to Matugen’s cache.
4. **Restart your session** or reload individual applications (e.g. `polybar`, `waybar`, `hyprland reload`) to pick up the new configuration.

## Repo Layout
- `hypr/` – Hyprland configuration split into `hyprland-config/`, monitor layouts, colors, idle/lock settings, and helper scripts.
- `bspwm/` – Window manager config with per-mode scripts (`modes/`), helpers, and the main `bspwmrc`.
- `polybar/` – Shared colors, modules, and scripts (including the clipboard-to-GPT helper and update notifications).
- `waybar/` – Waybar configuration and CSS theme generated via Matugen, with a custom power button wired to `scripts/power_menu.sh`.
- `eww/` – Optional control-center and glance widgets, including the toggleable `control_center` panel plus agenda / next-event / now-playing surfaces.
- `matugen/` – Template files and `config.toml` describing how colors propagate across Hyprland, Waybar, Kitty (auto-reloads via USR1), Dunst, and greetd/nwg-hello.
- `rofi/`, `kitty/`, `dunst/` – Application-specific themes.
- `scripts/` – Utility scripts such as `power_menu.sh` (logout/shutdown/reboot via Waybar/Rofi), `now_playing.sh` (current track for Hyprlock), and Bluetooth/wallpaper helpers.
- `greetd/`, `nwg-hello/` – Login screen configuration; Matugen generates `greetd.css` into `/var/cache/matugen/`, setup links it into `/etc/nwg-hello`, and installs a shared `base.conf` there for Hypr parity.
- `tools/` – Rust utilities such as `session-manager` and `launcher`.
- `tools/self-improve/` – Prompt assets for the login-time Codex automation.
- `zshrc` – Oh-My-Zsh setup, aliases, toolchain initializers, and environment sourcing.

## Customization Notes
- **Monitor naming**: Update `hypr/hyprland-config/desktop.conf` and the files under `hypr/monitor_layouts/` or `bspwm/modes/` if your outputs differ (`DP-5`, `DVI-D-2`, `HDMI-0`, etc.).
- **Hyprland modes**: Run the scripts from `hypr/scripts/modes/` to swap monitor layouts (`default.sh`, `game.sh`, `game_external.sh`). Each overwrites `~/.config/hypr/monitors.conf`.
- **BSPWM modes**: Trigger the scripts in `bspwm/modes/` (e.g. `all_monitors.sh`, `external_only.sh`) to adjust workspace assignments, padding, and Polybar/Nitrogen behavior.
- **Theme generation**: After installing Matugen, run it to regenerate color schemes across Waybar, Hyprland, Kitty, Dunst, and greetd/nwg-hello using the templates in `matugen/templates/`. Kitty reloads automatically via a USR1 signal once its colors file is written.
- **Greetd/NWG-Hello**: `setup.sh` links `greetd/config.toml`, installs `nwg-hello/hyprland.conf`, installs `hypr/hyprland-config/base.conf` to `/etc/nwg-hello/base.conf`, and symlinks `/etc/nwg-hello/nwg-hello.css` to the Matugen-generated cache at `/var/cache/matugen/greetd.css`. Backups of existing files are created with timestamps.
- **Rofi wrapper**: `setup.sh` links `scripts/rofi_wrapper.sh` to `~/.local/bin/rofi`, so custom Rofi scripts can always call `rofi` while still applying the Niri-specific theme automatically.
- **Control center**: Toggle it manually with `scripts/control_center.sh toggle`. The panel is backed by `scripts/control_center.sh` and the status polls in `eww/scripts/control_center_status.sh`.
- **System palette**: Press `Super+B` in Hyprland or Niri to open the Rofi quick-actions menu provided by `scripts/system_palette.sh`.
- **Session locking**: `scripts/session_lock.sh` is the shared entry point for keyboard shortcuts, the control center, and the power menu. It prefers `hyprlock` on Hyprland, otherwise falls back to the current logind session lock or a direct `hyprlock` invocation when available.
- **Optional Spotify service**: `setup.sh` links `systemd/user/auto-spotify.service` and enables it only when `spotify` and `pactl` are available. Set `AUTO_ENABLE_SPOTIFY_SERVICE=0` before running `setup.sh` to skip that step.
- **Codex self-improvement**: Hyprland, Niri, and bspwm startup now call `scripts/start_codex_self_improve_service.sh`, which imports the live session environment, ensures `codex-self-improve.service` is linked into `~/.config/systemd/user/`, and starts it. The service runs `scripts/codex_self_improve.sh` at most once per boot and no more than once per `CODEX_SELF_IMPROVE_COOLDOWN_HOURS` hours (24 by default), skips dirty worktrees, writes logs under `~/.local/state/codex-self-improve/`, and auto-commits successful changes with a `chore(self-improve): ...` message. Use `CODEX_SELF_IMPROVE_DISABLED=1` to turn it off, `CODEX_SELF_IMPROVE_AUTO_COMMIT=0` to keep successful changes uncommitted, or run `~/.dotfiles/scripts/codex_self_improve.sh --force` for a manual pass.
- **Shell tweaks**: Edit `AURHELPER` and plugin lists directly in `zshrc`; aliases assume tools such as `eza`, `bat`, `dust`, and `devour` are installed.
- **Polybar GPT module**: Requires `sgpt` and `xclip`. Send `USR1` to the module’s process (e.g. clicking the Polybar module) to transform clipboard prompts and return generated text.
- **Session manager**: Build `tools/session-manager` if you want to save and reapply monitor profiles across X11, Hyprland, and Niri sessions.

## License
No explicit license has been provided. Treat the contents as personal reference unless you have permission to reuse them.
