# Friendly UI Typography Design

## Goal

Replace the monospace typography on the active Wayland desktop surfaces with a
friendlier proportional typeface while preserving icon rendering and leaving
terminal and code-oriented typography unchanged.

## Typography Roles

- Use `Cantarell` as the primary interface font for Luma, Waybar, Eww, Dunst,
  Hyprlock, and greetd.
- Use `Symbols Nerd Font` only as Waybar's fallback for private-use icon glyphs.
- Preserve existing font sizes, spacing, and component dimensions.
- Preserve the stronger lock-screen clock hierarchy using Cantarell's available
  bold weight rather than a monospace-specific family.

## Scope

Update source configuration and templates only:

- `matugen/templates/luma.css`
- `waybar/style.css`
- `eww/eww.scss`
- `matugen/templates/dunstrc`
- `hypr/hyprlock.conf`
- `matugen/templates/greetd.css`
- `matugen/UI_STYLE.md`

Generated Matugen outputs are rendered for live verification but are not
committed. Kitty, editors, scripts, Polybar, and Cairo Dock remain unchanged.

## Compatibility

Cantarell and Symbols Nerd Font are installed on the target system. Waybar's
font stack places Cantarell first so ordinary text is proportional, followed by
Symbols Nerd Font so existing icons continue to resolve. Surfaces that render
only text use Cantarell directly.

## Verification

Extend the existing UI contract test to assert the typography roles and reject
the old JetBrains Mono assignments within the active Wayland UI scope. Follow a
red-green cycle, render the affected Matugen templates, run component syntax and
configuration checks, then inspect Luma and Waybar live for readable text,
working icons, and stable layout.
