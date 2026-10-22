# Shared desktop design and interaction

The core Wayland shell uses an industrial design: opaque surfaces, square geometry,
structural separators, and restrained morphing motion. This covers Vigil, Luma,
notifications, OSD, and compositor window decoration. Lock screen, greeter,
TV mode, and application interiors retain their existing styling for this pass.

- Spacing: 4, 8, 12, 16, and 24 px.
- Geometry: 0 px corner radius for panels, rows, controls, and compositor frames.
  Circular application artwork may keep its intrinsic shape.
- Structure: one continuous 44 px bar at the top edge of every monitor. Quick
  settings attaches to the right edge directly below the bar; Luma stays centered.
  Notifications keep 10 px clearance below the bar.
- Borders: 1 px structural separators, with 2 px accent focus indicators.
  Separate settings sections with dividers rather than nested cards.
- Opacity: visible shell backplates are fully opaque. Internal tints use opaque
  color mixes. Transparent layer-shell stage windows remain necessary around
  morphing content; they do not receive pointer input outside visible shapes.
- Depth: flat fills, no gradients, glow, or ambient shadows.
- Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback;
  use 0xProto for clocks, workspace numbers, resource readings, and shortcut hints.
- Color: Matugen supplies wallpaper-derived neutral surfaces and primary accents.
  `primary` marks focus and selection, `tertiary` marks warnings, and `error`
  marks critical, failed, urgent, or destructive states. Inactive borders use
  `outline_variant`. Supporting text uses `on_surface_variant`.

Vigil's `cockpit_*` aliases and Luma's `luma_*` aliases use the same Material
roles. Edit templates before generated files. Fallback styles follow the same
opaque, square design. Theme generation sends Vigil SIGHUP; Luma also watches
its generated theme so warm instances update after wallpaper changes.

Motion preserves spatial continuity: panels and notifications still morph from
and back into their originating bar controls. Vigil uses its existing `calm`
critically damped springs: no elastic overshoot, squash-and-stretch, or oscillation.
Interrupted morphs reverse smoothly. Hover and focus colors take 160 ms; launcher
and window arrivals use short eased transitions without tilt or bounce. Infinite
motion is reserved for pending or urgent states. Avoid idle decorative animation.

Hyprland owns Luma's layer entry and exit. Vigil owns its bar, quick-settings,
and toast transitions, with compositor animation disabled for those namespaces.
GTK controls respect reduced motion; compositor animations can separately be
turned off with `animations = { enabled = false }`. Vigil retains `bold` as an
optional configuration mode; this desktop selects `experience.motion = "calm"`.

Every monitor retains a visible search button. Left-click opens Luma there;
right-click opens window search. Query preservation, Ctrl+1 through Ctrl+5,
Enter, Escape, launcher toggling, and monitor selection remain unchanged.
Quick settings closes on a repeated general toggle; requesting a specific page
switches there first if needed.

Renderer-specific limitations may require equivalent values rather than exact
syntax. Document any deliberate visual exception next to the affected rule.
