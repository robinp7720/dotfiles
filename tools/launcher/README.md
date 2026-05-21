# dot-launcher

Native Rust launcher for this dotfiles repo.

## Features
- Desktop application launcher with icon-theme support
- Active window switching on Hyprland and Niri
- Predictive ranking based on local activation history
- Indexed file search through `localsearch`
- Native password-store actions through `pass`
- SSH host search from `~/.ssh/config`, `known_hosts`, and `known_hosts.old`
- Command runner with `$PATH` suggestions
- Browser bookmark search from Firefox and Chromium-family profiles
- Recent file search from `~/.local/share/recently-used.xbel`
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

Search prefixes:
```text
bookmark: rust docs
recent: report
```

## Notes
- Predictive history is stored as plain JSON in `~/.local/state/dot-launcher/predictions.json`.
- File search requires the `localsearch` CLI to be installed and indexed.
- Bookmark search reads Firefox `places.sqlite` through `sqlite3` when available and Chromium-family `Bookmarks` JSON directly.
- Recent file search reads local `file://` entries from `recently-used.xbel`.
- Window switching uses `hyprctl clients -j` on Hyprland and `niri msg windows --json` on Niri.
- Password search reads entry names from `PASSWORD_STORE_DIR` or `~/.password-store`.
- In password mode or `pass:` queries, a non-existing entry name shows an add row that generates a new password and saves optional username/email and URL metadata. If the clipboard contains a URL, the empty launcher and password mode can add the URL host directly and store the full URL automatically.
- Pressing Enter on a password result autotypes username, Tab, and password into the previously focused window without submitting the form.
- Password mode and `pass:` queries show action rows for autotype, copy password, copy username, type password, type username, and inspected metadata actions.
- Password entries use the standard `pass` format: first line is the password, with optional `key: value` metadata after it. Username keys are `user`, `username`, or `email`; otherwise the entry basename is used.
- URL, OTP, and custom autotype rows appear after choosing the inspect action for an entry. OTP actions require `pass-otp`.
- Autotype uses `wtype` on Wayland and `xdotool` on X11. Copying uses `wl-copy` on Wayland and `xclip` on X11, with secrets passed through stdin.
- Copied password data expires after `PASSWORD_STORE_CLIP_TIME` seconds, defaulting to 15 seconds.
- Web search defaults to DuckDuckGo. Override it with `DOT_LAUNCHER_SEARCH_URL`.
- SSH sessions launch through `~/.dotfiles/scripts/launch_kitty.sh -e ssh <host>`.
