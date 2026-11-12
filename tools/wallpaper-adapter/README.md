# Luma dotfiles wallpaper adapter

This binary implements Luma's version-1 JSON wallpaper protocol for this desktop.
It keeps Hyprland/Niri IPC, awww, systemd, Matugen and the per-output config format
out of Luma itself. `setup.sh` builds and links the executable, and adds adapter
settings only when Luma has no configured wallpaper integration. Existing Luma
preferences are preserved; an initial config backup is retained beside config.json.

- Backend command: `["~/.local/bin/luma-wallpaper-dotfiles"]`
- Optional theme command: `["~/.local/bin/luma-wallpaper-dotfiles", "theme"]`
- Images: `~/.dotfiles/wallpapers`
- State: `scripts/awww_wallpapers.conf`, overridden by `AWWW_WALLPAPER_CONFIG`

The backend reads the compositor's connected outputs and reports saved wallpaper
choices as `configured`. Apply validates the selected output, waits for `awww img`,
atomically updates that output's choice, then restarts `awww-wallpaper.service` so
its hotplug watcher reads the new mapping. Other outputs and `theme=` are retained.
The service uses `--no-theme`: startup/hotplug restores images without regenerating
colors. Existing generated palettes persist across sessions.

The theme hook invokes the watcher script's independent `--theme-only` path,
which runs Matugen and publishes greeter assets without setting monitor wallpapers.
Only after generation succeeds does it save `theme=`. `regenerate` uses the saved
theme image (or the default image). A failed publication may leave newly generated
desktop colors in place; it retains the previous greeter generation and reports
failure. Wallpaper and theme operations use a shared config lock.

Requests are JSON on stdin; responses are JSON on stdout. Errors go to stderr
with a nonzero exit status. Luma enforces operation deadlines. A timeout or service
restart failure can occur after the image/config changed; inspect state before
retrying. This adapter's line-oriented config cannot represent paths containing
`#`, CR, or LF, so it rejects those before applying.

Build/test:

```sh
cargo test --locked --manifest-path tools/wallpaper-adapter/Cargo.toml
bash tests/wallpaper_adapter_test.sh
```

The protocol and generic Luma settings are documented in
[`tools/launcher/docs/wallpapers.md`](../launcher/docs/wallpapers.md).
