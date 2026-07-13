# Floating-Islands Control Center Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development or superpowers:executing-plans to
> implement this plan task-by-task.

**Goal:** Redesign the Wayland cockpit bar as dark floating islands with one
compact control center and essential quick controls.

**Architecture:** Extend the existing typed source/store/action pipeline, then
replace independent popover specs with a stable unified dashboard widget tree.
Keep geometry in tracked CSS and Matugen colors limited to semantic states.

**Tech Stack:** Rust 2024, GTK4, gtk4-layer-shell, zbus, Bash contracts, Matugen.

## Global Constraints

- Work only on `feat/wayland-cockpit-bar`.
- Keep the 44px exclusive zone and Hyprland/Niri support.
- Use direct process argument arrays; never use shell expansion.
- Treat a missing real backlight as healthy unavailable state.
- Keep Waybar available for rollback until both compositor checks pass.

## Tasks

- [ ] Add typed brightness and Wi-Fi-radio state plus essential control actions,
  using failing parser and router tests first.
- [ ] Replace independent system popovers with one `ControlCenterSpec`, stable
  GTK dashboard, and 150ms debounced sliders, using failing spec/coordinator
  tests first.
- [ ] Render primary three-island and reduced two-island surfaces, including
  focused-title fallback, using failing surface tests first.
- [ ] Apply fixed dark-neutral CSS and semantic-only Matugen accents, protected
  by a failing theme contract test.
- [ ] Run format, clippy, Rust, shell, desktop, release, and live three-monitor
  verification; preserve rollback until Hyprland and Niri acceptance pass.
