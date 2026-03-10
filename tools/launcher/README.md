# dot-launcher

Native Rust launcher for this dotfiles repo.

## Features
- Desktop application launcher with icon-theme support
- Indexed file search through `tracker3`
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
dot-launcher --mode files
dot-launcher --mode ssh
```

## Notes
- File search requires the `tracker3` CLI to be installed and indexed.
- Web search defaults to DuckDuckGo. Override it with `DOT_LAUNCHER_SEARCH_URL`.
- SSH sessions launch through `~/.dotfiles/scripts/launch_kitty.sh -e ssh <host>`.
