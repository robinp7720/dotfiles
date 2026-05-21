# Luma

Luma is a unified command palette for Linux desktops.

It helps you launch apps, switch windows, search files, run commands, manage passwords, open bookmarks, and trigger common desktop actions from one fast overlay. The current Rust package and binary name are still `dot-launcher`.

## Features

- Application launcher with icon-theme support
- Active window switching on Hyprland and Niri
- Predictive ranking based on local activation history
- File search through `localsearch`
- Password-store search and native `pass` actions
- On-the-fly password creation with optional username, email, and URL metadata
- SSH host search from `~/.ssh/config`, `known_hosts`, and `known_hosts.old`
- Command runner with `$PATH` suggestions
- Browser bookmark search from Firefox and Chromium-family profiles
- Recent file search from `~/.local/share/recently-used.xbel`
- Web search through the default browser
- `libqalculate` integration through `qalc`

## Requirements

Luma is a desktop utility for Linux and expects a working GTK 4 environment.

Optional integrations:

- `localsearch` for file search
- `pass` for password search and creation
- `pass-otp` for OTP inspection actions
- `wtype` on Wayland or `xdotool` on X11 for password autotype
- `wl-copy` on Wayland or `xclip` on X11 for clipboard actions
- `hyprctl` for Hyprland window switching
- `niri` for Niri window switching
- `sqlite3` for Firefox bookmark search
- `qalc` for calculator queries

## Usage

Run from source:

```bash
cargo run --release --manifest-path tools/launcher/Cargo.toml
```

Or run the installed binary directly:

```bash
dot-launcher
```

Optional dedicated modes:

```bash
dot-launcher --mode commands
dot-launcher --mode windows
dot-launcher --mode files
dot-launcher --mode pass
dot-launcher --mode ssh
```

## Search Syntax

Luma understands a few lightweight prefixes:

```text
bookmark: rust docs
recent: report
pass: github/work
ssh: web-server
```

In the default mode, bare text is searched across the unified result set. Password queries also support adding new entries:

- If a `pass:` query names an entry that does not exist yet, Luma offers to create it.
- If the clipboard contains a URL, Luma can prefill the new password entry from the host name and store the full URL automatically.
- New password entries are generated locally and can include optional username/email metadata.

## Password Workflow

Password entries use the standard `pass` format:

- First line: the password
- Additional lines: `key: value` metadata

Recognized username keys are `user`, `username`, and `email`. If none of those are present, the entry basename is used.

Password results expose native actions for:

- Autotype
- Copy password
- Copy username
- Type password
- Type username
- Inspect metadata

Pressing Enter on a password result autotypes username, Tab, and password into the previously focused window without submitting the form.

## Notes

- Predictive history is stored as plain JSON in `~/.local/state/dot-launcher/predictions.json`.
- File search requires `localsearch` to be installed and indexed.
- Bookmark search reads Firefox `places.sqlite` through `sqlite3` when available and Chromium-family `Bookmarks` JSON directly.
- Recent file search reads local `file://` entries from `recently-used.xbel`.
- Window switching uses `hyprctl clients -j` on Hyprland and `niri msg windows --json` on Niri.
- Password search reads entry names from `PASSWORD_STORE_DIR` or `~/.password-store`.
- Autotype uses `wtype` on Wayland and `xdotool` on X11. Copying uses `wl-copy` on Wayland and `xclip` on X11, with secrets passed through stdin.
- URL, OTP, and custom autotype rows appear after choosing the inspect action for an entry. OTP actions require `pass-otp`.
- Copied password data expires after `PASSWORD_STORE_CLIP_TIME` seconds, defaulting to 15 seconds.
- Web search defaults to DuckDuckGo. Override it with `DOT_LAUNCHER_SEARCH_URL`.
- SSH sessions launch through `~/.dotfiles/scripts/launch_kitty.sh -e ssh <host>`.
