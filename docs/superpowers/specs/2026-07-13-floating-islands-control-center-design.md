# Floating-Islands Bar and Unified Control Center Design

## Summary

Replace the cockpit bar's full-width colored shell and independent system
popovers with a permanently dark-neutral floating-island bar and one unified
control center. Wallpaper-derived Matugen colors are reserved for selected,
enabled, charging, warning, and error states.

## Bar Surfaces

The primary output keeps the existing 44px exclusive zone and renders three
36px islands. The left island contains output-local workspaces and the active
application. The center is reserved for the existing priority context card and
falls back to the focused window title when no context is selected. The right
island presents condensed keyboard, resources, network, audio, power, and clock
state without permanent per-item borders.

Secondary outputs use two islands: output-local workspaces, app, and title on
the left; critical warning and clock on the right. They do not expose the
control center.

## Unified Control Center

Any primary-output status item opens the same approximately 420px dashboard and
focuses the matching section. The dashboard contains Wi-Fi, Bluetooth, audio
and mute, and power-profile quick controls; volume and optional brightness
sliders; optional media controls; compact keyboard, resources, battery,
calendar, and timer summaries; and a context-sensitive Luma footer.

Volume, mute, Wi-Fi, Bluetooth power, and brightness are the only new direct
controls. Network selection, device selection, and detailed settings remain in
Luma. Escape, outside click, output changes, and opening Luma close the panel.
Action failures stay beside the affected control until retry or close.

## State, Actions, and Availability

Brightness is available only when `brightnessctl --class=backlight` finds a
real backlight device; keyboard and network LEDs are never treated as displays.
Absence of a backlight is a healthy unavailable state. Volume and brightness
are clamped to 0-100% and slider writes are debounced by 150ms.

Actions use direct argument arrays through `wpctl`, `nmcli`, `bluetoothctl`, and
`brightnessctl`; no shell interpolation is permitted. The GTK dashboard and
status islands are created once and updated in place so source refreshes cannot
reparent live widgets.

## Visual Contract

- Shell: fixed smoked charcoal near `rgba(15, 20, 28, 0.84)`.
- Text: `#eef5f8`; supporting text: `#9eacb4`.
- Geometry: 12px island radius, 18px panel radius, low-contrast neutral border.
- Matugen: semantic accents only; generated surface roles never style ordinary
  shells or inactive controls.
- Existing context priority, dismissal, Hyprland/Niri behavior, and Waybar
  rollback path remain unchanged.

## Acceptance

Tests cover parsing, exact process arguments, unavailable brightness, state and
serde compatibility, context fallback, primary/reduced surface specs, unified
panel coordination, action errors, debounce behavior, and CSS role boundaries.
Live acceptance requires primary and reduced screenshots on the three-monitor
Hyprland layout, equivalent Niri behavior, working controls, and no new GTK
parenting assertions in the service log.
