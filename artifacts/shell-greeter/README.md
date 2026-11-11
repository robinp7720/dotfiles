# Native greeter verification

Verified on 2026-11-11 in an isolated Niri compositor nested in headless Weston.
The host desktop and its login service were not restarted.

- `login.png`: initial account/password view with the actual Matugen theme-source
  wallpaper, square panel, 24-hour clock, and installed session chooser.
- `authentication-error.png`: simulated failed authentication and keyboard retry.
- `additional-prompt.png`: a second, visible authentication prompt.
- `keyboard-focus.png`: keyboard traversal reaches the final power action.
- `scaled-footer.png`: at 2× scale (640×400 logical output), Tab scrolls the panel
  to reveal the complete power-control row.
- `power-confirmation.png`: destructive action confirmation scrolls into view;
  Cancel receives initial focus. All preview power actions are simulated.
- `fallback.png`: missing image/palette and a missing preferred output still yield
  a usable login surface. This preview also had GTK animations disabled.

Nine Rust unit tests passed, covering PAM conversations, cancellation, failed
launches, disconnection, state persistence, configuration, desktop entry parsing,
X11 argument wrapping, session fallback, and preview isolation. Clippy with
warnings denied, formatting, the release build, shell syntax, and the theme,
setup, wallpaper, desktop, Hyprland Lua, and asset-publication contracts passed.

The real GTK client was also exercised against a fake greetd socket. It submitted
two prompted answers, preserved argv and session environment, stayed alive until
the launch acknowledgement, then persisted only the account/session selection and
exited. No real authentication or desktop startup happened in that test. The
reusable harness is `tests/greeter_wayland_smoke.py`.

GTK emitted no CSS or application warnings in these previews. The isolated Niri
backend logged its expected software-rendering/EGL fallback warnings.

The release binary and configuration were installed as real, root-owned files.
The greeter account was verified able to discover sessions, read the published
assets, and write its private state directory. A system copy of 0xProto removes
the clock font's former dependency on the desktop user's home. Installed files match repository
sources. The original greetd daemon remained active with its original start time.
Backup: `/var/backups/shell-greeter/20261111-192321-611782`.

Activation awaits a greetd restart after ending the current session, or a reboot.
Real Hyprland/Niri/X11 login, return-to-greeter, physical output hotplug, and actual
power actions remain acceptance checks for that session boundary. The installed
nwg-hello files were retained for rollback; the greeter guide documents restoration.
