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
plus **GTK 4 Layer Shell** (`gtk4-layer-shell`, also used by Vigil and Luma),
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

After readiness checks, the desktop fades to black over 240 ms on every active
output. Display, audio and workspace changes happen behind this curtain. The first
rendered TV Home frame releases it with a 320 ms fade while Home arrives with a
small 12 px slide. HDMI resynchronization can still briefly blank the TV itself.
The curtain never takes focus or pointer input; startup rollback or a lost owner
releases it, with a 15-second safety limit if the shell never renders. Missing
transition support falls back to the normal startup path. `TV_MODE_REDUCED_MOTION=1`
skips the handoff animation; the desktop fade also respects GTK's animation setting.

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
| Focus intro / next-episode actions | D-pad up from seeking, then A | Up, then Enter |
| Select audio, subtitles, quality, or volume | Control row, then A | Control row, then Enter |
| Stop and return to details | B | Escape |
| Cycle subtitles, including off | X | X opens menu |
| Cycle audio | Y | Y opens menu |
| Cycle original / 1080p / 720p / auto | Left shoulder | Q opens menu |
| Change volume | Volume buttons | + / − |
| Skip current intro/recap | Right shoulder or Skip intro / recap button | N |
| Play next episode | Episode actions, then A | E or Next episode button |
| Next chapter | Control row, then A | Control row, then Enter |
| Show TV home / return to playback | Hold Guide / B | Home / Escape |

Controls fade during playback and reappear on input. Playback continues behind
Home; the resume action returns to the video. The timeline also supports mouse seeking.
The playback overlay uses a Netflix-style layout: white transport icons, a slim
red timeline, remaining time, and outlined episode actions over a dark gradient.
Controller focus or pointer hover reveals each icon's label. Audio, subtitle, and
quality choices open in a compact panel with plain selectable rows. Series/episode
context and the upcoming episode remain visible without a strip of text buttons.
Skip intro / recap appears only inside a valid marker supplied by Oblecto and stays
accessible while the main controls are hidden. It never falls back to a chapter jump;
Next chapter is a separate control and is disabled when no later chapter exists.
Missing markers do not produce guessed intro skips.

Next episode follows the adjacent episode in series order, including on rewatches,
and is available independently of the autoplay preference. It saves the current
position and releases the old session before starting the next, resumes that
episode's saved position, and asks for a source when multiple versions exist.
Unavailable adjacent episodes are not silently skipped. Episode titles respect
the account's spoiler preferences. Automatic continuation at the end retains the
account's autoplay and still-watching preferences.

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

Home features the selected title's artwork, synopsis and details action above compact
Oblecto, Steam and Kodi launchers and continue-watching shelves. The sidebar expands
into labelled navigation when focused; up/down stays in the sidebar, and right returns
to content. Movie and series pages use large titles, quiet secondary actions, a clear
play/resume or episode action, version selection, and season tabs. Browsing retains
its virtualized grid, pagination and saved position.

The shell shares the player's black-and-white visual language: charcoal surfaces,
white focus and primary actions, red watch-progress indicators, restrained corners,
and Inter typography. Steam and Kodi use their installed icons from
`/usr/share/icons/hicolor`, with built-in vector fallbacks when unavailable.
Search, profile selection and sign-in use neutral fields and square profile avatars.
The controller keyboard shows the text being edited and masks password previews.
Settings uses labelled rows, including the existing hold-to-sleep/shutdown actions.
Artwork is confined to Home and title details; utility screens remain plain.

TV mode deliberately uses this fixed high-contrast palette rather than wallpaper
colors for focus and playback controls. The Matugen theme bridge remains available
to components. Focus takes 160–180 ms, pages arrive with a 16 px slide and fade
in 280 ms, and artwork crossfades in 600 ms. The navigation rail, playback menus,
intro/next-episode actions, keyboard, tooltips and notices have short, reversible
entrance and exit transitions. Closing panels stop accepting input immediately.
Settings → Motion disables motion and finishes active transitions immediately for the
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

The offline visual fixture renders Home, navigation, catalogue and empty states,
movie/series details, episode grids, search, keyboard, profiles, sign-in and settings
at desktop and minimum window sizes. Its artwork and account data are synthetic:

```bash
mkdir -p /tmp/tv-mode-shell-qa
QT_QPA_PLATFORM=offscreen /usr/lib/qt6/bin/qmltestrunner -input tools/tv-mode/tests/visual
```

Screenshots are saved in `/tmp/tv-mode-shell-qa` for visual inspection.

For software decoding (including isolated Xvfb playback checks), set
`TV_MODE_SOFTWARE_VIDEO=1`. Normal playback uses automatic hardware decoding.
The native HLS smoke test requires FFmpeg, Xvfb, xdotool, and ImageMagick:

```bash
cargo build --manifest-path tools/tv-mode/Cargo.toml
xvfb-run -a -s '-screen 0 1600x900x24' python tools/tv-mode/tests/native_player_smoke.py
xvfb-run -a -s '-screen 0 1600x900x24' python tools/tv-mode/tests/native_player_smoke.py --episodes
```

It serves a generated clip from a local Oblecto fixture and checks real rendered
frames, pause/seek, quality changes, Home/return, engine reuse, and progress saving
on shutdown. The episode scenario also checks intro boundaries, paused skipping,
next-episode resume, and final-progress/deletion ordering before the next creation.
It uses temporary state and software decoding. Add `--artifacts /tmp/tv-mode-qa`
to retain screenshots and the player log for visual review.
