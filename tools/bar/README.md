# Cockpit Bar

## Prerequisite release build

Build the supervised binary before installing the desktop wiring:

```bash
cargo build --manifest-path tools/bar/Cargo.toml --release
```

`setup.sh` only links `%h/.local/bin/cockpit-bar` when
`tools/bar/target/release/cockpit-bar` already exists.

## Install

```bash
./setup.sh
systemctl --user daemon-reload
```

Hyprland and Niri restart `cockpit-bar.service` at session startup after their
environment import steps. The previous Waybar startup lines stay in the tracked
configs as commented fallback entries.

## Rollback

Immediate session rollback back to Waybar:

```bash
systemctl --user stop cockpit-bar.service || true
systemctl --user reset-failed cockpit-bar.service || true
rm -f ~/.config/systemd/user/cockpit-bar.service
rm -f ~/.local/bin/cockpit-bar
rm -f ~/.config/cockpit-bar
systemctl --user daemon-reload
~/.config/waybar/launch.sh --replace
```

Tracked config rollback in this repository:

```bash
git checkout 2187ecb^ -- setup.sh hypr/hyprland-config/startup.conf hypr/hyprland-config/base.conf niri/config.kdl tools/bar/README.md
rm -f systemd/user/cockpit-bar.service tools/bar/tests/desktop_contract.sh
```
