# Shared desktop design and interaction

Wayland system surfaces use a visible, Matugen-driven glass treatment. Colors
come from Material roles; geometry and depth follow this shared scale:

- Spacing: 4, 8, 12, 16, and 24 px.
- Radii: 10 px for compact controls, 14 px for windows, bar islands, and rows,
  and 18 px for cards, Luma, and quick settings. Filter pills remain fully rounded.
- Borders: 1 px using a low-contrast `outline_variant` role.
- Opacity: bar islands use 86%; Luma and quick settings use 94-96% so text
  stays readable over busy windows. Inner cards and controls use lighter tints.
- Shadows: ambient cards use `0 12px 28px` at 25% black; overlays use
  `0 20px 48px` at 36% black.
- Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback; use `on_surface_variant` for supporting text.
- States: `primary` marks focus and selection, `tertiary` marks warnings, and
  `error` is reserved for critical, failed, urgent, or destructive states.
  Inactive window borders use `outline_variant`, leaving focus easy to locate.

Vigil's `cockpit_*` aliases and Luma's `luma_*` aliases come from the same
Matugen Material roles. Edit templates before generated files. Theme generation
sends Vigil SIGHUP to update existing surfaces; Luma reads the theme on opening.

Motion communicates an action: an expressive spring on arrival, a short press response,
and a quicker departure. Hover and focus changes take 160 ms, presses settle in
180 ms, and surfaces arrive in 320 ms and depart in 180 ms. Workspace changes
keep their direction with limited travel. Infinite motion is reserved for live
pending or urgent states. Avoid continuous decorative motion while idle.
Luma and windows arrive from 92% scale with a small overshoot. Selecting a filter
gives its pill a single 240 ms bounce; hovering a result lifts and tilts its icon.
Quick settings springs in from the bar corner, and compact controls compress on
press. These gestures add expression without extending the opening durations.

Hyprland owns Luma's layer entry and exit. Vigil owns its bar and quick-settings
animations, with compositor animation disabled for those two namespaces. This
avoids applying two entrance transforms to one surface. GTK controls respect
reduced motion; compositor motion can separately be disabled with
`animations = { enabled = false }` in `hypr/hyprland-config/base.lua`. Vigil also offers
`experience.motion = "calm"` and `experience.ambient_motion = "off"`.

Every monitor has a visible search button. Left-click opens Luma there;
right-click opens window search. Luma keeps the query when switching filters,
supports Ctrl+1 through Ctrl+5, and keeps focus inside its controls. Enter opens
the selected result and Escape closes the surface. Repeating a launcher shortcut
closes that launcher; a different shortcut switches its purpose. Quick settings
close on a repeated general toggle; requesting a specific page switches there
first if needed. Opening either surface respects the chosen monitor.

The motion vocabulary follows the supported
[Hyprland animation tree](https://wiki.hypr.land/0.54.0/Configuring/Animations/)
and [GTK CSS properties](https://docs.gtk.org/gtk4/css-properties.html).

Renderer-specific limitations may require equivalent values rather than exact
syntax. Document any deliberate visual exception next to the affected rule.
