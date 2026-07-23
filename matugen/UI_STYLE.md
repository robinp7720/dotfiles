# Balanced Glass UI contract

Wayland system surfaces use a visible, Matugen-driven glass treatment. Colors
come from Material roles; geometry and depth follow this shared scale:

- Spacing: 4, 8, 12, 16, and 24 px.
- Radii: 10 px for compact elements, 14 px for controls and rows, 18 px for
  cards, and 18 px for Vigil's cockpit shells. Pills remain fully rounded.
- Borders: 1 px using a low-contrast `outline_variant` role.
- Opacity: Vigil's cockpit shells stay at 58%, ordinary modules and cards use
  48-70%, and selected, warning, or status-bearing surfaces may rise to 82%.
- Shadows: ambient cards use `0 12px 28px` at 25% black; overlays use
  `0 20px 48px` at 36% black.
- Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback; use `on_surface_variant` for supporting text.
- States: `primary` marks focus and selection, `tertiary` marks warnings, and
  `error` is reserved for critical, failed, urgent, or destructive states.

Renderer-specific limitations may require equivalent values rather than exact
syntax. Document any deliberate visual exception next to the affected rule.
