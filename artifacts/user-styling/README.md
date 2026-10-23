# User-defined shell styling

Verified 2026-10-23 in a nested Niri Wayland session at 1280x800.

- `vigil-default.png`: original rounded geometry and pre-redesign dotfiles CSS,
  with the new native style settings omitted.
- `vigil-custom.png`: current dotfiles CSS and square, edge-attached geometry
  selected explicitly by `[style]` in `bar/config.toml`.
- `luma-default.png`: original built-in rounded, translucent styling with no user
  CSS, in front of the custom Vigil preview.
- `luma-custom.png`: Matugen theme plus persistent `Luma/style.css` overrides.

Luma's persistent CSS was edited while running: a diagnostic border appeared
without restarting. Deleting the override removed it; recreating it restored
the border without restarting. No CSS parsing errors
were observed. Isolated D-Bus portals emitted environment/accessibility warnings.

Vigil passed 319 tests (one additional test ignored); Luma passed 175 tests.
Both release builds passed, alongside Vigil's strict Clippy check, desktop and
UI-theme contracts, setup build/failure-isolation tests, and shell syntax checks.
Luma retains existing unused/dead-code build warnings.

The installed Vigil service was restarted with the new release and dotfiles
configuration. Luma's persistent style was linked into the live configuration;
its existing preferences and generated theme were preserved.
