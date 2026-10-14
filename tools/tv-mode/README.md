# TV Mode

`tv-mode` is a controller-first Hyprland session for the LG TV on
`Unknown-2`. It temporarily makes the TV the only active output, switches HDMI
audio, and presents Steam and Kodi from a fullscreen couch interface.

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

The home screen uses 4K30. Steam switches to 1080p120 and the performance
power profile; Kodi uses 4K30 and balanced. D-pad or the left stick moves
focus, A selects, B returns to the active app, and holding Guide for 1.5
seconds reopens the home screen. Arrow keys, Enter, and Escape are available
as keyboard fallbacks.

Display modes are matched by resolution and the nearest advertised refresh
rate within 0.2%, so nominal 30/60/120 Hz settings also accept fractional
29.97/59.94/119.88 Hz timings. The selected advertised mode is applied and
shown on the home screen; unsupported resolutions or rates still fail before
changing the desktop.

Steam startup remains in the gaming profile while its launcher updates; a
temporary updater window closing does not count as leaving Big Picture.
After Kodi's window closes, Home returns even if Kodi's launcher is still
generating a crash report. Compositor query failures do not count as app exits.

Sleep and shutdown require a two-second hold. Sleep uses the shared lock flow
before suspending. Exit remains unavailable while Kodi or a fullscreen
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
