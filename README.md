# Robin's Dotfiles

Personal configuration for a Wayland desktop built around Hyprland and Niri. The repository installs compositor configuration, a generated visual theme, lock and login surfaces, helper scripts, and three native tools: Vigil, Luma, and TV mode.

## Highlights

- **Hyprland and Niri** share the same launch, lock, wallpaper, and cockpit workflows.
- **Vigil** is the always-visible desktop cockpit and notification server. It provides workspaces, urgent context, quick settings, timers, media, notifications, and monitor-aware control-center placement.
- **Luma** is the command palette for applications, files, windows, passwords, system controls, and Vigil handoffs.
- **Dynamic Matugen themes** cover Vigil, Luma, Hyprland, Hyprlock, Niri, Kitty, TV mode, greetd, and nwg-hello.
- **Focused-output locking** renders a privacy-bounded Hyprlock dashboard with calendar, timer, media, network, power, and notification-count context.
- **Controller-first TV mode** temporarily moves the session to the TV and restores the previous monitor and service state afterwards.

## Requirements

The configuration targets Arch Linux or a derivative. Core runtime dependencies are:

- Desktop: `hyprland`, `niri`, `hypridle`, `hyprlock`, `hyprsunset`, `kitty`
- Login: `greetd`, `nwg-hello`
- Desktop services: `pipewire`, `wireplumber`, `networkmanager`, `bluez`, `power-profiles-daemon`, `polkit-gnome`
- Helpers: `playerctl`, `pactl`, `wpctl`, `brightnessctl`, `notify-send`, `grim`, `slurp`, `wl-copy`
- Theming and wallpaper: `matugen`, `awww`
- Builds: a Rust toolchain for Vigil, Luma, and TV mode

Optional integrations include `khal` or `gcalcli` for calendar context, `pass` for passwords, Steam and Kodi for TV mode, Spotify for Bluetooth auto-launch, and Codex for self-improvement automation.

## Installation

```bash
git clone --recurse-submodules https://github.com/robinp7720/dotfiles.git ~/.dotfiles
cd ~/.dotfiles
cargo build --release --manifest-path tools/bar/Cargo.toml
cargo build --release --manifest-path tools/launcher/Cargo.toml --bin Luma
cargo build --release --manifest-path tools/tv-mode/Cargo.toml
./setup.sh
```

`setup.sh` links supported configuration under `~/.config`, exposes built binaries under `~/.local/bin`, installs user units, reloads the user daemon, and installs greetd/nwg-hello files when `sudo` is available. Existing non-symlink paths are moved to timestamped backups. Links managed by older versions for removed desktop stacks are cleaned up without touching foreign paths.

Use `INSTALL_SYSTEM_CONFIG=0 ./setup.sh` for a user-only pass that does not prompt for `sudo` or update greetd/nwg-hello under `/etc`.

Restart the session after initial installation. Individual changes can be applied with `hyprctl reload`, `systemctl --user restart vigil.service`, or `systemctl --user restart awww-daemon.service awww-wallpaper.service`.

## Repository Layout

- `hypr/`, `niri/` — compositor configuration and monitor layouts
- `bar/` — tracked Vigil runtime configuration and CSS
- `kitty/` — terminal configuration
- `matugen/` — theme source templates; generated outputs should not be edited directly
- `greetd/`, `nwg-hello/` — login screen configuration
- `scripts/` — wallpaper, lockscreen, calendar, media, and session helpers
- `systemd/user/` — Vigil, wallpaper, TV-mode, Spotify, and Codex units
- `tools/bar`, `tools/launcher` — Vigil and Luma Git submodules
- `tools/tv-mode` — controller-first couch-session tool
- `tv-mode/` — TV-mode configuration and theme
- `tests/` — desktop, theme, lock, wallpaper, and service contracts

## Everyday Controls

| Shortcut | Action |
| --- | --- |
| `Super+Space` | Toggle Luma search |
| `Super+Tab` | Toggle open-window search |
| `Super+Shift+Space` | Toggle desktop controls in Luma |
| `Super+P` | Toggle password search |
| `Super+B` | Toggle quick settings on the focused output |
| `Super+Shift+N` | Toggle the notification inbox |
| `Super+L` | Lock through the shared session-lock lifecycle |
| `Super+T` | Start TV mode |
| `Super+Arrow` | Focus a neighboring window |
| `Super+Shift+Arrow` | Move a window in that direction |
| `Super+Ctrl+Arrow` | Resize the current split |
| `Super+grave` | Return to the previous workspace |

The search button on every bar opens Luma on that monitor; right-click it for
windows. Luma's All, Apps, Windows, Controls, and Files filters preserve the search
text and are also available with `Ctrl+1` through `Ctrl+5`. Use the arrow keys and
Enter to select a result, or Escape to close. Repeating a shortcut closes its
surface; requesting a different mode or settings page switches to it.

Luma's Controls filter includes Vigil's live settings and actions. Opening Luma
closes an open quick-settings panel, and selecting a settings result hands control
back to Vigil. Matching windows appear in the initial search ranking beside apps.
Status checks have time limits and run off the UI thread.

Luma stays ready for 30 seconds after closing for faster repeat openings, then
exits automatically. Adjust `ui.idle_timeout_seconds` in
`~/.config/Luma/config.json` (`0` exits immediately). Closed windows release their
search state and stop polling. Vigil wakes immediately for launcher and settings
requests; live status updates reuse existing search results.

Vigil's control center is also available from the command line:

```bash
vigil control-center open
vigil control-center toggle
vigil control-center open --context notifications --output DP-3
vigil control-center close
```

## Configuration Notes

- Monitor layouts live in `hypr/monitor_layouts/`, `hypr/monitors.lua`, and `niri/outputs.kdl`. Update connector names when hardware changes.
- `scripts/awww_wallpapers.conf` maps wallpapers to outputs. `awww-daemon.service` owns the sole wallpaper daemon; `awww-wallpaper.service` applies images and follows output changes.
- Matugen writes generated palettes to ignored runtime paths. Run `matugen image <wallpaper> -c ~/.config/matugen/config.toml` to refresh them.
- Colors, geometry, motion, and reduced-motion options are documented in [the desktop design contract](matugen/UI_STYLE.md). Vigil reloads its theme automatically after Matugen generation.
- `scripts/session_lock.sh` closes transient Vigil UI, refreshes lock-safe context, and uses `swaylock` only as an emergency fallback.
- Vigil owns `org.freedesktop.Notifications`; normal notification clients such as `notify-send` continue to work.
- Set `AUTO_ENABLE_SPOTIFY_SERVICE=0` while running setup to skip the optional Bluetooth Spotify service.
- Set `CODEX_SELF_IMPROVE_DISABLED=1` to disable login-time self-improvement or `CODEX_SELF_IMPROVE_AUTO_COMMIT=0` to retain successful changes without committing them.

## Validation

```bash
bash -n setup.sh scripts/*.sh hypr/scripts/modes/*.sh
for test_file in tests/*.sh scripts/tests/*.sh; do bash "$test_file"; done
niri validate -c niri/config.kdl
Hyprland --config hypr/hyprland-config/desktop.conf --verify-config
Hyprland --config hypr/hyprland-config/laptop.conf --verify-config
cargo test --manifest-path tools/bar/Cargo.toml
cargo test --manifest-path tools/launcher/Cargo.toml
cargo test --manifest-path tools/tv-mode/Cargo.toml
```

Visible changes should also be checked with screenshots on the primary and reduced outputs. Long-session changes should verify that Vigil retains one audio subscriber and that awww owns one wallpaper layer per output.

## License

No explicit license has been provided. Treat the contents as personal reference unless you have permission to reuse them.
