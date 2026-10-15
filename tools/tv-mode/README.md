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
(`qt6-base qt6-declarative qt6-wayland` on Arch), plus the existing session tools.
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
used by the fullscreen home screen; unsupported resolutions or rates still fail before
changing the desktop.

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

The Oblecto card opens the integrated library. The default server is
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

Playback uses **mpv 0.41+**. mpv receives
session-scoped media URLs over a private IPC socket, never the account token or a
URL on its command line. Oblecto negotiates direct, remux, or transcoded HLS output.
Progress is sent every ten seconds, on transitions, and before session deletion.
Audio/subtitle/quality changes preserve position and pause state. An expired
playback session is recreated once; a failed direct decode gets one HLS fallback.
Errors remain visible instead of endlessly retrying.

| During playback | Controller | Keyboard |
| --- | --- | --- |
| Play/pause | A | Space |
| Seek 10 seconds | D-pad left/right | Left/right |
| Volume | D-pad up/down | mpv's 9/0 |
| Stop and return to details | B | Escape |
| Cycle subtitles, including off | X | X |
| Cycle audio | Y | Y |
| Cycle original / 1080p / 720p / auto | Left shoulder | Q |
| Skip current intro/recap, or next chapter | Right shoulder | N |
| Show TV home / return to playback | Hold Guide / B | `tv-mode home` / Escape |

Library navigation uses D-pad/left stick and A/B, or arrows/Enter/Escape. Press A on a text field to open the controller-operated keyboard with uppercase
and symbols; B dismisses it and restores field focus. Sign-in passwords are never saved. “Remember me” stores the token
through `secret-tool` in the desktop Secret Service keyring (install `libsecret`
and run a compatible keyring). If it is unavailable, sign-in still works for the
current session. Only server/account metadata goes in
`$XDG_STATE_HOME/tv-mode/oblecto-profile.json`; sign-out removes the saved token.

`[oblecto].player` defaults to `mpv` and can name another mpv executable. The
client uses the native API, not Jellyfin compatibility endpoints. Socket.IO remote
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
`src/player.rs` manages mpv and Oblecto playback sessions. Qt runs on the main
thread; HTTP and decoding work run off it. Stale navigation results are ignored.
Artwork is cached in a private temporary directory with a 128-entry limit; older
entries are evicted individually and visible covers are reloaded on demand. The
cache is removed on sign-out/exit. Image URLs are never reused between accounts.

The large 16:9 canvas, Inter typography, opaque cinema backdrop, and larger
18–34 px radii deliberately adapt the desktop style for viewing from a sofa.
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
source selection, and power press/release.
Use `tv-mode preview` for visual checks and playback without switching displays.
Use `tv-mode preview --no-controller` in isolated Xvfb tests so the test window
does not consume controller input intended for your desktop preview.
