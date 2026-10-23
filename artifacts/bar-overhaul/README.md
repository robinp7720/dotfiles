# Bar overhaul

Verified 2026-10-23.

- `workspaces.png`: live Hyprland bar crop, showing full-width active workspace
  marker, inset launcher, aligned labels, and continuous neutral backplate.
- `compact-bar.png`: synthetic Vigil control-center scenario in an isolated
  Niri/Weston session at 1280×800. The entire primary bar fits at 1280 px;
  quick settings attaches at y=44. The screenshot contains fixture data.
- Live DP-3, DP-2, and Unknown-2 each retain a 44 px top reservation.
- UI theme contracts, Vigil desktop contracts, shell syntax and diff checks pass.
- No CSS parser errors. Live accessibility-bus registration errors remain;
  nested rendering also reports software EGL/portal warnings.

The rail now has a near-neutral wallpaper-tinted backplate, full accent workspace
selection, distinct search/time controls, a quieter center and no vertical cell
dividers. Resource readings and keyboard layout remain in quick settings.
Personal styling stays in dotfiles; application defaults are unchanged.
