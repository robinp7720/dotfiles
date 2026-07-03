# Soft Glass UI contract

Wayland system surfaces use a restrained, Matugen-driven glass treatment. Colors
come from Material roles; geometry and depth follow this shared scale:

- Spacing: 4, 8, 12, 16, and 24 px.
- Radii: 10 px for compact elements, 14 px for controls and rows, 18 px for
  cards, and 22 px for overlay shells. Pills remain fully rounded.
- Borders: 1 px using a low-contrast `outline_variant` role.
- Opacity: text-bearing surfaces stay between 90% and 96% opaque.
- Shadows: ambient cards use `0 12px 28px` at 25% black; overlays use
  `0 20px 48px` at 36% black.
- Typography: JetBrainsMono Nerd Font, with `on_surface_variant` for supporting
  text.
- States: `primary` marks focus and selection, `tertiary` marks warnings, and
  `error` is reserved for critical, failed, urgent, or destructive states.

Renderer-specific limitations may require equivalent values rather than exact
syntax. Document any deliberate visual exception next to the affected rule.
