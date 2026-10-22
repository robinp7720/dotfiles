# Industrial core shell verification

Verified on 2026-10-22.

- `launcher.png`: Luma in an isolated Wayland session; opaque panel, square rows,
  underline filters, keyboard selection, and live Matugen theme reload.
- `quick-settings.png`: synthetic Vigil preview at 1280×800.
- `quick-settings-scrolled.png`: keyboard focus reaches the footer on that short
  output; the panel scrolls within the area below the 44 px bar.
- `notifications-osd.png`: a local test notification and volume OSD in an isolated
  Niri session with separate notification state and D-Bus session.

Vigil: 318 tests passed, one ignored. Luma: 175 tests passed. Release builds,
UI theme/desktop contracts, compositor binding parity, shell syntax checks,
Hyprland Lua validation, and Niri config validation passed.

Live Hyprland reload reported no config errors. Its three bar surfaces measured
3840×44, 2160×44, and 1920×44, each reserving exactly 44 px. The physical session
was locked, so interactive rendering was checked in the isolated session instead.
Repeated panel toggles at 80 ms intervals, Escape, and audio-page opening completed.
One GTK frame-timing warning appeared during the isolated rapid-toggle check;
there were no CSS parser errors. Luma retains four existing dead-code warnings.

Niri's installed CLI returns a map of outputs. Vigil now accepts both that format
and the previous list format, with an integration test covering workspace focus.
