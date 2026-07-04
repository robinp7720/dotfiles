# Balanced Glass Recalibration

## Problem

The Soft Glass implementation is visually indistinguishable from an opaque
theme. Luma renders its shell at 94-96% opacity and most child surfaces at
90-96%. Waybar retained its previous 86% shell and 92% module opacity, and it
has no Hyprland blur layer rule.

The observed `XDG_SESSION_TYPE=X11` is isolated to the Codex process. Kitty,
Waybar, and the user systemd environment all report `wayland`, so normal
Hyprland launches do not require a Luma backend workaround.

## Design

- Reclassify the theme as Balanced Glass and update the documented opacity
  contract.
- Render Luma overlay shells at 68-74% opacity, controls and cards at 58-72%,
  and selected or status-bearing surfaces at 70-82% for legibility.
- Render Waybar's shell at 58%, ordinary modules at 48-60%, and media/event
  modules at 62-70%. Keep warnings, critical states, and active selections more
  opaque.
- Add Hyprland `blur` and `ignore_alpha 0.20` layer rules for the `waybar`
  namespace. Retain the existing Luma layer rules.
- Preserve typography, dimensions, commands, result ordering, and interaction
  behavior.

## Verification

- Render Matugen output and verify GTK/Waybar CSS startup without parser errors.
- Verify Hyprland and Niri configuration parsing.
- Capture live Luma and Waybar screenshots over a detailed wallpaper; wallpaper
  color and texture must visibly pass through both shells while text remains
  readable.
- Run the Luma test suite and release build, then reload Hyprland and Waybar.
