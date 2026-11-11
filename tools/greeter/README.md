# Shell greeter

Rust/GTK4 login surface for greetd, styled with the desktop's industrial shell.
Authentication stays in greetd/PAM; the greeter only sends protocol requests and
renders their replies. It supports arbitrary secret/visible prompts, information,
errors, cancellation, remembered accounts/sessions, and installed Wayland/X11
sessions. Session commands retain their desktop-entry arguments; no shell parses
credentials or desktop entry commands. X11 entries use the existing startx path.

## Development

```sh
cargo test --locked --manifest-path tools/greeter/Cargo.toml
cargo clippy --locked --manifest-path tools/greeter/Cargo.toml --all-targets -- -D warnings
cargo build --release --locked --manifest-path tools/greeter/Cargo.toml
tools/greeter/target/release/shell-greeter --config shell-greeter/config.toml --check-config
tools/greeter/target/release/shell-greeter --config shell-greeter/config.toml --preview
```

Run previews in an isolated layer-shell compositor, since the panel takes keyboard
focus. Preview never opens GREETD_SOCK, writes remembered state, starts desktops,
or invokes power commands. Enter `wrong` for an authentication error, `otp` for a
second visible prompt, or `disconnect` for a simulated service failure. Other
responses complete the demonstration. Success previews the fade to black and
back, then leaves the preview open.

For an end-to-end GTK test against a fake greetd socket, run
`tests/greeter_wayland_smoke.py` with the three isolated compositor environment
variables documented in its header. This verifies the actual widget-to-protocol
path without PAM or real session startup.

Start the isolated Hyprland itself with `HYPRLAND_NO_SD_VARS=1`,
`HYPRLAND_NO_SD_TARGET=1`, and `HYPRLAND_NO_SD_NOTIFY=1`, and a minimal config
without desktop startup commands. A separate Wayland socket alone does not
isolate user systemd: without these flags, starting/exiting a nested compositor
can replace the live activation environment and stop the live desktop services.

## Session handoff

After greetd acknowledges session launch, the greeter fades every output to black
over 240 ms before exiting. Failed authentication or launch never fades the UI.
The greeter compositor and Hyprland desktop both use black as their uncovered
background, including after Matugen regeneration, to bridge compositor teardown.

The fade alone cannot prevent a monitor losing its signal. Aquamarine 0.16.0
explicitly disables KMS outputs during backend destruction. The greeter's
`shell-greeter-compositor` launcher opts into a private patched build that skips
that shutdown disable, allowing its existing `drmModeCloseFB` cleanup to leave
the last frame scanning out until the next compositor replaces it. Ordinary
monitor disconnection still disables the output. This changes neither the
system library nor the desktop session's backend.

The private build is pinned to upstream revision
`b77d67648336223a6c8fb88844ae316e7b72b574`. The launcher checks fingerprints of
Hyprland, the system Aquamarine library and the build's shared dependencies before
loading it; a mismatch falls back to stock Aquamarine. Rebuild and reinstall the
private backend after compatible package upgrades. Other Aquamarine versions use
the stock backend until the patch is reviewed for that version. This removes one
known signal interruption; driver and VT handoff may still cause a modeset, so
uninterrupted physical output must be checked with a real login on the target GPU.

Hyprland's desktop startup helper runs `shell-greeter --desktop-reveal` before
restarting the wallpaper and bar services. This mode does not load greeter
configuration, contact greetd or read/write authentication state. It reports
`ready` on stdout after drawing an opaque curtain on every discovered output;
the startup script waits at most two seconds for that handshake. Once awww has
an image and Vigil has a visible surface on every active output for two successive
checks, the curtain fades away over 320 ms. After four seconds it reveals anyway.
The transparent input region and absent keyboard focus keep it purely cosmetic.
New outputs receive matching curtains while the helper is running.

GTK's reduced-motion setting or `SHELL_SESSION_REDUCED_MOTION=1` skips both
animations and the desktop readiness wait. Turning GTK animations off during a
transition releases it immediately. The greeter setting belongs to the greeter
account; a user-session preference alone does not change that separate account.
Desktop reveal currently targets Hyprland with awww and Vigil; other selected
sessions still receive the greeter fade-out but use their own startup behavior.

`tests/session_transition_wayland_smoke.py` exercises real GTK surfaces in an
isolated Hyprland using fixture readiness queries, captured fade frames, the
missing-service timeout and reduced motion. It requires grim and Python Pillow.
This and the fake-greetd test do not establish physical VT/display handoff timing;
that requires a real logout/login acceptance pass.
An isolated fake-greetd capture of the fade is saved in
[`artifacts/shell-greeter/handoff-fade.png`](../../artifacts/shell-greeter/handoff-fade.png).

The panel prefers DP-3, falls back to the first connector in sorted order, and
moves when outputs disappear. It adapts to scaling and scrolls at short heights.
The wallpaper fills every output, with an opaque fallback if it cannot be read.
Configuration is validated before connecting to Wayland or greetd.

## Deployment and rollback

`setup.sh` builds the greeter before installation and uses the narrow installer
below for root-owned files. `INSTALL_SYSTEM_CONFIG=0` skips all system changes.
For a greeter-only update after building:

```sh
scripts/build_greeter_handoff.sh
sudo bash scripts/install_greeter.sh "$USER"
scripts/awww_wallpaper_watcher.sh --greeter-only
```

The standalone executable lives in `/usr/local/bin`, the minimal compositor and
configuration in `/etc/shell-greeter`, and remembered state in the private,
greeter-owned `/var/lib/shell-greeter`. No executable or configuration required
before login is linked into a user's home. Setup copies the desktop owner's
0xProto font into `/usr/local/share/fonts/shell-greeter` so a user-only font
installation also works before login; Cantarell is supplied by the system. GTK4, GTK4 Layer Shell, greetd,
Hyprland/start-hyprland, the two interface fonts, and Python/GObject/GdkPixbuf for
asset publication are required. X11 additionally needs startx and Xorg.

Building the private backend also requires Git, CMake, Ninja, a C++23 compiler,
pkg-config and Aquamarine's development dependencies. The build script runs the
upstream non-display tests and a fake-KMS harness exercising the real backend:
stock shutdown disables outputs, opt-in shutdown retains them, and hotplug still
disables them; all paths release output objects and notify listeners. Run
`bash tests/greeter_handoff_launcher_test.sh` to check matching, stale and missing
dependency manifests. These tests never open a GPU or change the active session.
The installed library and its manifest live under
`/usr/local/lib/shell-greeter/handoff`, readable by the greeter without `/home`.
Renaming its `system.sha256` manifest disables the private backend for subsequent
greeter launches and restores the standard library without changing the UI.

The existing wallpaper watcher publishes a validated image/palette pair using an
atomic `current` symlink in `/var/cache/shell-greeter`. Only the desktop owner can
publish assets; the greeter only reads them. Generation failure retains the old
pair, including one previous generation. `--greeter-only` generates only this
palette and wallpaper; it does not reload the desktop. The pre-login process
resolves both asset paths to the same generation at startup.

Installation backs up prior system files under `/var/backups/shell-greeter` and
switches greetd's configuration last. It does **not** restart greetd or terminate
the active desktop. Restart greetd only after ending the desktop session, or
reboot to activate the new configuration. Authentication, real compositor
handoff, X11 startup, return to greeter, multi-monitor hotplug, and power actions
need a real login-session acceptance pass; previews do not prove these paths.

The nwg-hello package and its installed configuration remain available. To roll
back, restore `etc/greetd/config.toml` from the printed backup directory to
`/etc/greetd/config.toml`, then restart greetd from a TTY after ending the desktop
session (or reboot). This restores the previous greeter command; no PAM changes
are made by this project.
