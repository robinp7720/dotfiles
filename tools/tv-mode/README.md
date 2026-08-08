# TV Mode

`tv-mode` is a controller-first Hyprland session for the LG TV on
`HDMI-A-2`. It temporarily makes the TV the only active output, switches HDMI
audio, and presents Steam and Jellyfin from a fullscreen couch interface.

## Build and install

```bash
cargo build --manifest-path tools/tv-mode/Cargo.toml --release
./setup.sh
```

`setup.sh` links the release binary, configuration, and `tv-mode.service`.
The service is started on demand and is not enabled at login.

## Usage

Open Luma and search for `tv`, press `Super+T`, or run:

```bash
tv-mode start
```

The home screen uses 4K60. Steam switches to 1080p120 and the performance
power profile; Jellyfin uses 4K60 and balanced. D-pad or the left stick moves
focus, A selects, B returns to the active app, and holding Guide for 1.5
seconds reopens the home screen. Arrow keys, Enter, and Escape are available
as keyboard fallbacks.

Sleep and shutdown require a two-second hold. Sleep uses the shared lock flow
before suspending. Exit remains unavailable while Jellyfin or a fullscreen
Steam/game window is active.

## Recovery

The service restores displays, workspace assignments, audio, power profile,
Hypridle, and Vigil state when it stops. If a crash leaves the session in TV
mode, run:

```bash
tv-mode recover
```

Runtime state is stored with user-only permissions under
`$XDG_RUNTIME_DIR/tv-mode`; tracked `hypr/monitors.conf` is never rewritten.
