# Session Manager

A Rust-based tool to manage desktop sessions and monitor layouts based on connected hardware. It "learns" your hardware configurations (Home, Work, Mobile) and applies them dynamically, regardless of which port you plug your monitors into.

## Features

- **Hardware Fingerprinting:** Identifies monitor setups using Serial Numbers or EDID data (stable IDs), not ephemeral port names (like `DP-1`).
- **Profile Learning:** Save your current layout with `session-manager save <NAME>`.
- **Auto-Application:** Detects the connected hardware and applies the matching profile (Hyprland, Niri, or X11/Bspwm).
- **Hybrid Support:** Works with Hyprland, Niri (via `niri msg`), and X11 (via `xrandr`).

## Installation

```bash
cd tools/session-manager
cargo build --release
cp target/release/session-manager ~/.local/bin/
```

## Usage

### 1. List Monitors
Check how the tool sees your monitors and their stable IDs.
```bash
session-manager list
```

### 2. Learn a Layout
Configure your monitors manually (using `arandr`, `wdisplays`, or manual config) exactly how you want them. Then, save the state:
```bash
session-manager save "Home Desk"
```

### 3. Apply a Layout
When you plug in your dock/monitors, run:
```bash
session-manager apply
```
This will:
1. Detect connected monitors.
2. Find a matching profile (by hashing the hardware set).
3. Generate and apply the config (rewrites `~/.config/hypr/monitors.conf` or runs `xrandr`).

### 4. Custom Commands
You can edit `~/.config/session-manager/config.toml` to add commands to run when a profile is applied (e.g., restarting bars, setting wallpapers).

```toml
[profiles."<HASH>"]
name = "Home Desk"
commands = [
    "systemctl --user restart waybar",
    "nitrogen --restore"
]
```
