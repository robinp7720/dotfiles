# Shell refinement verification

Verified 2026-10-23 in the live Hyprland session.

- `launcher.png`: native Wayland Luma search, keyboard focus, tinted selection,
  unframed application icons, filters and footer; captured with grim.
- `bar.png`: cropped live bar showing inset controls and workspace underline.
- All three bar surfaces retain a 44 px height and reservation.
- UI theme and Vigil desktop contracts, shell syntax, and diff checks passed.
- Generated Luma theme from the source template using the configured wallpaper
  and saturation preference. Vigil reloaded with SIGHUP; Luma rendered with
  GDK_BACKEND=wayland. Neither reported CSS parser errors.
- Existing Vigil frame-timing warnings preceded this reload. An initial Luma
  launch used XWayland; the final screenshot and validation use native Wayland.
