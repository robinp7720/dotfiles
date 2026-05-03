# dot-launcher

Native Rust launcher for this dotfiles repo.

## Features
- Desktop application launcher with icon-theme support
- Active window switching on Hyprland and Niri
- Predictive ranking based on local activation history
- Indexed file search through `localsearch`
- Password-store search through `pass`
- SSH host search from `~/.ssh/config`, `known_hosts`, and `known_hosts.old`
- Command runner with `$PATH` suggestions
- Web search through the default browser
- `libqalculate` integration through `qalc`

## Usage
```bash
cargo run --release --manifest-path tools/launcher/Cargo.toml
```

Optional dedicated modes:
```bash
dot-launcher --mode commands
dot-launcher --mode windows
dot-launcher --mode files
dot-launcher --mode pass
dot-launcher --mode ssh
```

## Notes
- Predictive history is stored as plain JSON in `~/.local/state/dot-launcher/predictions.json`.
- File search requires the `localsearch` CLI to be installed and indexed.
- Window switching uses `hyprctl clients -j` on Hyprland and `niri msg windows --json` on Niri.
- Password search reads entry names from `PASSWORD_STORE_DIR` or `~/.password-store`, and copies the first line from `pass show <entry>`.
- Web search defaults to DuckDuckGo. Override it with `DOT_LAUNCHER_SEARCH_URL`.
- SSH sessions launch through `~/.dotfiles/scripts/launch_kitty.sh -e ssh <host>`.
