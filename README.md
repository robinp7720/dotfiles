# Robin's Dotfiles

Personal configuration for a Wayland desktop built around Hyprland and Niri. The repository installs compositor configuration, a generated visual theme, lock and login surfaces, helper scripts, and four native tools: Vigil, Luma, TV mode, and the shell greeter.

## Highlights

- **Hyprland and Niri** share the same launch, lock, wallpaper, and cockpit workflows.
- **Vigil** is the always-visible desktop cockpit and notification server. It provides workspaces, urgent context, quick settings, timers, media, notifications, volume/brightness and idle-lock displays, and monitor-aware control-center placement.
- **Luma** is the command palette for applications, files, windows, passwords, system controls, and Vigil handoffs.
- **Dynamic Matugen themes** cover Vigil, Luma, Hyprland, Hyprlock, Niri, Kitty, TV mode, and the native shell greeter.
- **Focused-output locking** renders a privacy-bounded Hyprlock dashboard with calendar, timer, media, network, power, and notification-count context.
- **Controller-first TV mode** temporarily moves the session to the TV and restores the previous monitor and service state afterwards.

## Requirements

The configuration targets Arch Linux or a derivative. Core runtime dependencies are:

- Desktop: `hyprland`, `niri`, `hypridle`, `hyprlock`, `hyprsunset`, `kitty`
- Login: `greetd`, the bundled Rust shell greeter, `python-gobject`, `gdk-pixbuf2`; `xorg-xinit` and `xorg-server` for X11 sessions
- Desktop services: `pipewire`, `wireplumber`, `networkmanager`, `bluez`, `power-profiles-daemon`, `polkit-gnome`
- Helpers: `bash`, `python`, `jq`, `playerctl`, `pactl`, `wpctl`, `brightnessctl`, `notify-send`, `grim`, `slurp`, `wl-copy`, `wl-paste`, `cliphist`
- Shell preview: `fzf`, `eza`, `bat`; setup installs the tracked `preview` helper
- Theming and wallpaper: `matugen`, `awww`
- Builds: a Rust toolchain, C/C++ compiler, and `pkg-config`; GTK 4 and gtk4-layer-shell development files for Vigil/Luma/the greeter; Evolution and Evolution Data Server development files for Luma's mail integration; Qt 6 Quick/OpenGL, libmpv, and libudev development files for TV mode

Optional integrations include `khal` or `gcalcli` for calendar context, `pass` for passwords, Steam and Kodi for TV mode, Spotify for Bluetooth auto-launch, and Codex for self-improvement automation.
The optional `hyprshade` timer uses the tracked `hypr/hyprshade.toml`. Install Hyprshade with its `vibrance` and `blue-light-filter` shaders before enabling it with `systemctl --user enable --now hyprshade.timer`. Hyprshade is currently absent on the audited machine, so its existing timer cannot apply a filter.

## Installation

```bash
git clone --recurse-submodules https://github.com/robinp7720/dotfiles.git ~/.dotfiles
cd ~/.dotfiles
./setup.sh
```

`setup.sh` initializes missing tool submodules and builds Vigil, Luma, `luma-mail-eds`, TV mode, and the shell greeter in release mode using their lockfiles. It also builds and installs the local Hyprland swipe-event plugin. Install the build dependencies listed above and the plugin development dependencies below before running setup. A build failure stops setup before it changes installed configuration or services.

After building, setup links supported configuration under `~/.config`, exposes binaries under `~/.local/bin`, installs user units, reloads the user daemon, and installs the root-owned greeter executable and greetd configuration when `sudo` is available. Greeter system backups live under `/var/backups/shell-greeter`; installation does not restart an active login session. Existing non-symlink paths are moved to timestamped backups. Links managed by older versions for removed desktop stacks are cleaned up without touching foreign paths.

Use `INSTALL_SYSTEM_CONFIG=0 ./setup.sh` for a user-only pass that does not prompt for `sudo` or update system greeter files. The new greetd configuration takes effect after a service restart or reboot. See [the greeter guide](tools/greeter/README.md) for previews and rollback.

### Hyprland gesture plugins

Three-finger swipes drive a Mission Control overview from hymission. Swiping up
gathers the windows of the current workspace below a row of workspaces along
the top; swiping down gathers the windows from every workspace on the monitor
under the cursor. Both track the fingers and settle on release, and swiping the
opposite way closes the overview. Horizontal swipes switch workspaces, inside
the overview as well.

hymission comes from a fork at `tools/hymission`
([robinp7720/hymission](https://github.com/robinp7720/hymission), tracking
[gfhdhytghd/hymission](https://github.com/gfhdhytghd/hymission) as `upstream`).
Upstream's two-sided gesture spans every monitor on its wide side, and its
overview always reserves room for window search. The fork adds a
`forceall_only_active_monitor` option that keeps the wide side on one monitor
and a `search_enabled` option that removes search and its header. With an
upstream build, the config falls back to the same scopes on discrete swipes
that do not follow the fingers, and search stays. Build the fork, its
`hymission-search-input` helper, and the local swipe-event plugin with:

```bash
./scripts/install_hyprland_plugins.sh
```

Plugins must be rebuilt against the headers matching the installed Hyprland.
The build requires `make`, CMake, a C++23 compiler, GTK 4, gtk4-layer-shell,
libadwaita, and development files for Hyprland, Pixman, libdrm, Pango/Cairo,
libinput, libudev, Wayland, xkbcommon, and Lua. Setup builds and installs only
the swipe-event plugin, so run the installer for hymission. Restart Hyprland
after installing plugins.

After upgrading Hyprland, rebuild the local plugins. The Lua config skips
plugins whose build ABI does not match the running compositor, including local
binaries without an `.abi` file; stale plugins can crash inside their static
initializers before their own version checks run. The installer checks
development headers and writes the ABI files only after successful builds.
A compatible local hymission build takes precedence over a Hyprpm copy.

Restart the session after initial installation. When migrating an existing Hyprland session from `.conf` to Lua, use `hyprctl reload full-reset` once to switch parsers. Individual changes can be applied with `hyprctl reload`, `systemctl --user restart vigil.service`, or `systemctl --user restart awww-daemon.service awww-wallpaper.service`.

## Repository Layout

- `hypr/`, `niri/` — compositor configuration and monitor layouts
- `bar/` — tracked Vigil runtime configuration and CSS
- `kitty/` — terminal configuration
- `matugen/` — theme source templates; generated outputs should not be edited directly
- `greetd/`, `shell-greeter/` — login screen configuration; `nwg-hello/` retains the legacy fallback
- `scripts/` — wallpaper, lockscreen, calendar, media, and session helpers
- `wallpapers/` — the four images used by the active wallpaper mapping
- `systemd/user/` — Vigil, wallpaper, TV-mode, Spotify, and Codex units
- `tools/bar`, `tools/launcher` — Vigil and Luma Git submodules
- `tools/hymission` — Git submodule with the Mission Control overview plugin fork
- `tools/logiops` — Git submodule with the Logitech device support fork; built and installed separately from `setup.sh`
- `tools/tv-mode` — controller-first couch-session tool
- `tools/greeter` — Rust/GTK4 login surface using greetd IPC
- `tv-mode/` — TV-mode configuration and theme
- `tests/` — desktop, theme, lock, wallpaper, and service contracts

## Everyday Controls

| Shortcut | Action |
| --- | --- |
| `Super+Space` | Toggle Luma search |
| `Super+Tab` | Toggle open-window search |
| `Super+Shift+Space` | Toggle desktop controls in Luma |
| `Super+P` | Toggle password search |
| `Super+/` | Search keyboard shortcuts (Niri: its own overlay) |
| `Super+Shift+V` | Search clipboard history |
| `Super+B` | Toggle quick settings on the focused output |
| `Super+Shift+N` | Toggle the notification inbox |
| `Super+L` | Lock through the shared session-lock lifecycle |
| `Super+T` | Start TV mode |
| `Super+Arrow` | Focus a neighboring window |
| `Super+Shift+Arrow` | Move a window in that direction |
| `Super+Ctrl+Arrow` | Resize the current split |
| `Super+grave` | Return to the previous workspace |
| `Super+F` / `Super+Shift+F` | Toggle maximized / fullscreen |
| `Super+V` | Toggle floating |
| `Super+Ctrl+Space` | Cycle keyboard layouts |
| `Print` / `Shift+Print` | Screenshot an area / the focused monitor (Niri keeps its own `Print` UI; `Super+Print` is shared) |

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

- Hyprland loads `hypr/hyprland.lua` and native Lua modules in `hypr/hyprland-config/` (Hyprland 0.55+). Desktop is the default; set `HYPRLAND_PROFILE=laptop` before starting Hyprland, or use `--config ~/.config/hypr/hyprland-config/laptop.lua`. Matugen generates `hypr/colors.lua` and reloads the palette. Vigil, Luma, TV mode, and display helpers use the Lua IPC API; rebuild the tools when migrating.
- Monitor layouts live in `hypr/monitor_layouts/`, `hypr/monitors.lua`, and `niri/outputs.kdl`. Update connector names when hardware changes.
- `scripts/awww_wallpapers.conf` maps wallpapers to outputs. `awww-daemon.service` owns the sole wallpaper daemon; `awww-wallpaper.service` applies images and follows output changes.
- Active wallpapers are bundled under `wallpapers/`; their filenames identify the original Wallhaven images. The older `hypr/hyprpaper.conf` is unused by the current startup path.
- Personal shell settings can remain in `~/.config/environment`. Setup preserves that file; it is not needed by the desktop helpers and should not be copied into Git with credentials.
- Matugen writes generated palettes to ignored runtime paths. Run `matugen image <wallpaper> -c ~/.config/matugen/config.toml` to refresh them.
- Colors, geometry, motion, and reduced-motion options are documented in [the desktop design contract](matugen/UI_STYLE.md). Vigil reloads its theme automatically after Matugen generation.
- `scripts/session_lock.sh` closes transient Vigil UI, refreshes lock-safe context, and uses `swaylock` only as an emergency fallback.
- Vigil owns `org.freedesktop.Notifications`; normal notification clients such as `notify-send` continue to work.
- Set `AUTO_ENABLE_SPOTIFY_SERVICE=0` while running setup to skip the optional Bluetooth Spotify service.
- Codex maintenance runs at compositor login, at most once successfully per boot with a default 24-hour cooldown; it is not a daily timer. It first invokes Codex to group and commit all existing changes with relevant messages (nested submodules before their parent pointers), then attempts one evidenced improvement in an independent checkout. Commit-only failures stop the improvement pass. Automation leaves commits local; publish them separately, pushing submodules before the parent.
- Unregistered nested Git checkouts cause maintenance to skip before invoking Codex. Register them as submodules or exclude standalone local checkouts in the containing repository's `.git/info/exclude`. Automation commits are unsigned because signing agents are unavailable in the sandbox; normal Git signing settings are unchanged.
- Improvements require a valid JSON report and independent checks before integration. The wrapper only fast-forwards a still-clean, unchanged live checkout. Failed or concurrent runs retain the isolated checkout for inspection. Supported automatic edits are shell helpers, user service units, selected compositor configs, and documentation; submodule and automation changes require a separate task.
- Persistent automation settings go in `~/.config/codex-self-improve/environment` as `NAME=value` lines (systemd EnvironmentFile syntax). Use `CODEX_SELF_IMPROVE_DISABLED=1` to disable it. `CODEX_SELF_IMPROVE_AUTO_COMMIT=0` (or manual `--no-commit`) skips dirty repositories and retains validated improvements in isolation, without changing the live checkout. The launcher also imports these settings from the login environment.
- Run `scripts/codex_self_improve.sh --force` to bypass the boot/cooldown gates. Optional settings include `CODEX_SELF_IMPROVE_MODEL`, `CODEX_SELF_IMPROVE_PROFILE`, `CODEX_SELF_IMPROVE_COOLDOWN_HOURS` (default 24), and `CODEX_SELF_IMPROVE_TIMEOUT_SECONDS` (default 1800 per agent/validation phase). The unit also has a two-hour total timeout.
- Inspect `~/.local/state/codex-self-improve/last-summary.txt` and `last-run.log`. Valid failure reports include the agent's explanation in the summary and failure notification. Every attempt has its own directory under `runs/`, including skipped attempts; JSON reports, validation output, and candidate patches are retained there. Successful temporary checkouts are removed, while failed/pending checkouts remain for manual review and cleanup. Runtime dependencies include Codex, Git, jq, and util-linux `flock`.

## Validation

```bash
bash -n setup.sh scripts/*.sh hypr/scripts/modes/*.sh
for test_file in tests/*.sh scripts/tests/*.sh; do bash "$test_file"; done
niri validate -c niri/config.kdl
Hyprland --config hypr/hyprland-config/desktop.lua --verify-config
Hyprland --config hypr/hyprland-config/laptop.lua --verify-config
cargo test --manifest-path tools/bar/Cargo.toml
cargo test --manifest-path tools/launcher/Cargo.toml
cargo test --manifest-path tools/tv-mode/Cargo.toml
cargo test --manifest-path tools/greeter/Cargo.toml
```

Visible changes should also be checked with screenshots on the primary and reduced outputs. Long-session changes should verify that Vigil retains one audio subscriber and that awww owns one wallpaper layer per output.

## License

No explicit license has been provided. Treat the contents as personal reference unless you have permission to reuse them.
