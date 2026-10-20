# Repository Guidelines

## Project Structure & Module Organization
This repository mirrors the Wayland desktop config it installs, built around Hyprland and Niri. `setup.sh` symlinks top-level directories such as `hypr/`, `niri/`, `kitty/`, `matugen/`, and `nvim/` into `~/.config`, links `bar/` to `~/.config/vigil`, and installs `greetd/` and `nwg-hello/` under `/etc` when `sudo` is available.

- `hypr/` — Hyprland Lua config. `hyprland.lua` picks a profile (`desktop` or `laptop`) from `hyprland-config/`; shared settings live in `base.lua`, `bindings.lua`, `startup.lua`, and `plugins.lua`. `monitors.lua` is written by nwg-displays and `hypr/scripts/modes/`; do not edit it by hand.
- `niri/` — Niri config. Keep its shortcuts in step with `hypr/hyprland-config/bindings.lua`; `tests/compositor_binding_parity_test.sh` checks the shared ones.
- `bar/` — Vigil runtime config (`config.toml`) and CSS.
- `tools/bar` (Vigil) and `tools/launcher` (Luma) are Git submodules with their own history; commit inside the submodule first, then bump it here. `tools/tv-mode` is a Rust + Qt Quick couch-session tool configured from `tv-mode/`.
- `scripts/` — shell helpers for screenshots, locking, wallpapers, media, and calendar context.
- `systemd/user/` — user services for Vigil, wallpapers, TV mode, Spotify, and Codex.
- `matugen/templates/` — theme sources. Edit templates, never generated outputs such as `bar/colors.css`, `hypr/colors.lua`, or `tv-mode/colors.css`. `matugen/UI_STYLE.md` defines the shared spacing, radii, opacity, and motion scale.

Build new tools in `tools/` when needed: Rust for complex logic, shell scripts for glue code and simple helpers.

## Build, Test, and Development Commands
- `./setup.sh` links configs, installs binaries into `~/.local/bin`, and reloads user `systemd`. `INSTALL_SYSTEM_CONFIG=0 ./setup.sh` skips `/etc`.
- `cargo build --release --manifest-path tools/bar/Cargo.toml` builds Vigil; use `tools/launcher/Cargo.toml --bin Luma` for Luma and `tools/tv-mode/Cargo.toml` for TV mode. `cargo test` with the same manifest runs each tool's tests.
- `bash -n setup.sh scripts/*.sh hypr/scripts/modes/*.sh tests/*.sh` performs a fast shell syntax check.
- `niri validate -c niri/config.kdl` checks the Niri config.
- Reload the component you changed: `hyprctl reload` (then `hyprctl configerrors`), `systemctl --user restart vigil.service`, or `systemctl --user daemon-reload`.
- Use screenshots (`grim -o <output>`) to verify visual changes to the launcher, bar, lock screen, and greeter.
- Ask for user feedback on behavior changes, especially for session, lock, and greeter flows.

## Coding Style & Naming Conventions
Use Bash for shell entrypoints with `#!/usr/bin/env bash`, `set -euo pipefail`, and consistently quoted variable expansions. Keep script names in `snake_case`, such as `now_playing.sh` and `session_lock.sh`. Shell files use 2-space indentation inside arrays, conditionals, and `case` arms. Hyprland Lua uses 4-space indentation and the `hl.*` API; give user-facing bindings a `description`. Rust uses standard `cargo fmt` formatting.

## Testing Guidelines
`tests/` holds shell contract tests for the desktop, theme, lock, wallpaper, TV mode, and compositor bindings; run the ones that cover the files you touched, for example `bash tests/vigil_desktop_contract.sh`. When a behavior change is intentional, update the contract alongside it. For config edits, also run the syntax checks above and exercise the affected path in a live session.

## Commit & Pull Request Guidelines
History follows Conventional Commit style with optional scopes, for example `fix(niri): ...`, `feat(hyprland): ...`, `style(bar): ...`, and `chore(vigil): bump submodule ...`. Keep commits focused on one subsystem. Always commit and push completed changes unless the user explicitly instructs otherwise. Commit and push submodule changes before committing and pushing their updated pointers in this repository. Pull requests should describe the affected desktop surface, list any new package or monitor-name assumptions, and include screenshots for visible launcher, bar, lock, or greeter changes.
