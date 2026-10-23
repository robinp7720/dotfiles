# TV Mode

`tv-mode` is a controller-first Hyprland session for the LG TV on
`Unknown-2`. It temporarily makes the TV the only active output, switches HDMI
audio, and presents Oblecto, Steam, and Kodi from a fullscreen couch interface.

The interface uses Qt Quick's GPU scene graph with artwork crossfades, animated
focus, a multi-row catalogue, and a controller-operated keyboard. It is not a GTK
application. Rust owns authentication, networking, input, playback, and session
recovery; a small C++ bridge exposes presentation state to QML.

## Build and install

Requires a C++17 compiler, pkg-config, **Qt 6 Base, Declarative, and Wayland**
and **libmpv 0.38+** (`qt6-base qt6-declarative qt6-wayland mpv` on Arch),
an OpenGL 3.3-capable graphics driver, and the existing session tools.
QML and artwork-free ambient visuals are embedded in the binary; no web server
or browser is needed. Inter is the preferred font, with the system sans fallback.


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
power profile; Kodi uses 4K30 and balanced. D-pad or the left stick moves
focus, A selects, B returns to the active app from Home, and holding Guide for 1.5
seconds reopens the home screen. Arrow keys, Enter, and Escape are available
as keyboard fallbacks.

Display modes are matched by resolution and the nearest advertised refresh
rate within 0.2%, so nominal 30/60/120 Hz settings also accept fractional
29.97/59.94/119.88 Hz timings. The selected advertised mode is applied and
used by the fullscreen home screen. Startup waits up to 30 seconds for the TV to
reconnect and advertise all configured modes, then up to 30 seconds for the
configured HDMI audio profile to become available, before changing the desktop. If
the TV remains unavailable or a mode is unsupported, startup fails with a message
in `journalctl --user -u tv-mode.service`; turn on the TV, select its connected
input, and retry.

Steam startup remains in the gaming profile while its launcher updates; a
temporary updater window closing does not count as leaving Big Picture.
After Kodi's window closes, Home returns even if Kodi's launcher is still
generating a crash report. Compositor query failures do not count as app exits.

Exit TV is visible in the header on every screen. B/Escape from Home returns to
the desktop when no entertainment app is active. View/Select exits from the TV
interface or Oblecto playback; Ctrl+Q also exits the interface. Exiting during
Oblecto playback saves progress and closes the player. Steam/Kodi retain their
controller inputs while in front and remain protected from accidental termination.

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

## Oblecto

The Oblecto card opens the movie library, or sign-in when no account is connected.
Continue-watching and catalogue shelves are also available directly on Home. The default server is
`https://oblecto.home.robindecker.me`; change `[oblecto].server` in
`~/.config/tv-mode/config.toml`, or enter another address on the sign-in screen.
HTTP/HTTPS deployment prefixes are supported. Choose a profile and enter its
password, or use username/password sign-in. Passwordless login is offered only
when the server explicitly permits it for that profile.

The client provides continue-watching and up-next shelves, a seven-column grid
for movies, series, search and saved lists, and a season picker with episode grids.
The catalogue loads more titles as you navigate or scroll; there are no page hops.
Back from a title restores the loaded catalogue, selected poster, and scroll position.
Only nearby grid cards are instantiated, and failed loads offer an inline retry
without discarding your place. It also provides favourites/watchlists, details,
numeric season/episode ordering,
watched actions, source selection, and resume/start-over. Missing artwork and
unavailable sources have fallback states. Account quality, audio/subtitle language,
and episode-spoiler preferences are applied. Automatic continuation runs after an
episode actually ends; multiple source versions require a choice. If the account's
still-watching prompt is enabled, continuation asks after 90 minutes without input.

Playback is embedded in TV mode, with a native video surface and controller-operated
controls. **libmpv** handles decoding; Oblecto supplies managed HLS streams, remuxing
or transcoding as needed. There is no separate player window. Session-scoped media
URLs travel over a private control socket and never enter QML or a command line.
Progress is sent every ten seconds, on transitions, and before session deletion.
Audio, subtitle, and quality changes preserve position and pause state. An expired
session is recreated once; errors remain visible instead of endlessly retrying.

| During playback | Controller | Keyboard |
| --- | --- | --- |
| Play/pause | A | Space |
| Seek 10 seconds | D-pad left/right | Left/right |
| Focus control row; return to seeking | D-pad down; up | Down; up |
| Select audio, subtitles, quality, or volume | Control row, then A | Control row, then Enter |
| Stop and return to details | B | Escape |
| Cycle subtitles, including off | X | X opens menu |
| Cycle audio | Y | Y opens menu |
| Cycle original / 1080p / 720p / auto | Left shoulder | Q opens menu |
| Change volume | Volume buttons | + / − |
| Skip current intro/recap, or next chapter | Right shoulder | N |
| Show TV home / return to playback | Hold Guide / B | Home / Escape |

Controls fade during playback and reappear on input. Playback continues behind
Home; the resume action returns to the video. The timeline also supports mouse seeking.

Library navigation uses D-pad/left stick and A/B, or arrows/Enter/Escape. Press A on a text field to open the controller-operated keyboard with uppercase
and symbols; B dismisses it and restores field focus. Sign-in passwords are never saved. “Remember me” stores the token
through `secret-tool` in the desktop Secret Service keyring (install `libsecret`
and run a compatible keyring). If it is unavailable, sign-in still works for the
current session. Only server/account metadata goes in
`$XDG_STATE_HOME/tv-mode/oblecto-profile.json`; sign-out removes the saved token.

The client uses the native Oblecto API. The old `[oblecto].player` executable
setting is no longer used. Socket.IO remote
casting, administrative tools, and viewing-statistics dashboards are outside this
client's current scope. Library shelves refresh on navigation; there is no
background realtime connection.

To inspect the interface without changing displays, audio, or power profiles:

```bash
tv-mode preview
```

Preview disables Steam/Kodi launching and power actions; Oblecto sign-in and
playback remain functional. The implementation follows the upstream
[desktop client guide](https://github.com/robinp7720/Oblecto/blob/master/docs/DESKTOP_CLIENTS.md),
[REST API](https://github.com/robinp7720/Oblecto/blob/master/docs/API.md), and
[response models](https://github.com/robinp7720/Oblecto/blob/master/docs/API_MODELS.md).


## Appearance and development

`qml/` contains the interface, `qt/bridge.cpp` the Qt host, `src/library.rs` the
asynchronous library state, and `src/platform.rs` controller/process monitoring.
`src/player.rs` manages the embedded decoder and Oblecto playback sessions;
`qt/bridge.cpp` owns libmpv and its OpenGL render context. Qt runs on the main
thread; HTTP and decoding work run off it. Stale navigation results are ignored.
Artwork is cached in a private temporary directory with a 128-entry limit; older
entries are evicted individually and visible covers are reloaded on demand. The
cache is removed on sign-out/exit. Image URLs are never reused between accounts.

Home puts three app tiles above the library shelves. A neutral background,
Inter typography, restrained corners, and white focus outlines keep navigation
readable from a sofa. Steam and Kodi use their installed icons from
`/usr/share/icons/hicolor`, with built-in vector fallbacks when unavailable.
Headings describe the content; titles retain their year, runtime, rating, and synopsis.
Focus uses the Matugen `primary` role read from `tv-mode/colors.css`; edit
`matugen/templates/tv-mode.css` for palette changes. Focus takes 160–180 ms,
pages arrive in 320 ms, and artwork crossfades in 600 ms. Decorative visuals
stay still while idle. Settings → Motion disables these animations for the
current session; `TV_MODE_REDUCED_MOTION=1` selects it at startup.

```bash
cargo test --manifest-path tools/tv-mode/Cargo.toml
cargo clippy --manifest-path tools/tv-mode/Cargo.toml --all-targets --no-deps -- -D warnings
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tools/tv-mode/tests/qml
bash tests/tv_mode_contract.sh
```

The QML tests exercise controller navigation, password keyboard entry, focus
preservation while artwork loads and pages append, grid navigation/virtualization,
restoring browse position, fast pagination responses, direct exit, episode grids,
source selection, player menus and controls, and power press/release.
Use `tv-mode preview` for visual checks and playback without switching displays.
Use `tv-mode preview --no-controller` in isolated Xvfb tests so the test window
does not consume controller input intended for your desktop preview.

For software decoding (including isolated Xvfb playback checks), set
`TV_MODE_SOFTWARE_VIDEO=1`. Normal playback uses automatic hardware decoding.
The native HLS smoke test requires FFmpeg, Xvfb, xdotool, and ImageMagick:

```bash
cargo build --manifest-path tools/tv-mode/Cargo.toml
xvfb-run -a -s '-screen 0 1600x900x24' python tools/tv-mode/tests/native_player_smoke.py
```

It serves a generated clip from a local Oblecto fixture and checks real rendered
frames, pause/seek, quality changes, Home/return, engine reuse, and progress saving
on shutdown. It uses temporary state and software decoding.
