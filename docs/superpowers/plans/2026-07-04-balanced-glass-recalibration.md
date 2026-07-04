# Balanced Glass Recalibration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Luma and Waybar visibly translucent and blurred while preserving text readability and all existing behavior.

**Architecture:** Keep Matugen as the color source and adjust only renderer opacity values. Add a Hyprland layer rule so Waybar receives the compositor's existing blur; do not change Luma backend selection because normal Hyprland children already inherit Wayland correctly.

**Tech Stack:** Matugen templates, GTK CSS, Waybar CSS, Hyprland configuration, Bash regression test.

---

### Task 1: Add a failing contract test

**Files:**
- Create: `tests/ui_theme_contract_test.sh`

- [ ] Add a Bash test that asserts Luma shell opacity `0.72/0.68`, Luma entry opacity `0.66`, Waybar shell opacity `0.58`, Waybar module opacity `0.52/0.60`, and Hyprland Waybar `blur` plus `ignore_alpha 0.20` rules.
- [ ] Run `bash tests/ui_theme_contract_test.sh` and confirm it fails against the current Soft Glass values.
- [ ] Commit the failing regression test with `test(ui): define balanced glass contract`.

### Task 2: Recalibrate the renderer values

**Files:**
- Modify: `matugen/templates/luma.css`
- Modify: `waybar/style.css`
- Modify: `hypr/hyprland-config/base.conf`
- Modify: `matugen/UI_STYLE.md`

- [ ] Change Luma shells to a `0.72` high-surface to `0.68` base-surface gradient, entries to `0.66`, selected rows to `0.78/0.70`, status rows to `0.70`, icon wells to `0.62`, and settings cards to `0.66`.
- [ ] Change Waybar shell opacity to `0.58`, ordinary modules to `0.52`, alternate modules to `0.60`, media/event modules to `0.68/0.64`, neutral disconnected modules to `0.52`, warning/charging/clock modules to `0.72`, power hover to `0.72`, and tooltips to `0.82`. Keep active, critical, and urgent states more opaque.
- [ ] Add `blur on` and `ignore_alpha 0.20` rules for namespace `waybar` beside the existing Luma layer rules.
- [ ] Update the shared UI contract from Soft Glass and 90-96% opacity to Balanced Glass and the new 48-82% hierarchy.
- [ ] Run `bash tests/ui_theme_contract_test.sh`, `Hyprland --verify-config --config hypr/hyprland-config/base.conf`, and `git diff --check`.
- [ ] Commit with `fix(ui): make glass surfaces visibly translucent`.

### Task 3: Render and verify live behavior

**Files:**
- Generated, ignored: `~/.config/Luma/theme.css`, `waybar/colors.css`

- [ ] Render the current `#8394a8` palette through a temporary Matugen config targeting Luma and Waybar.
- [ ] Start Waybar and Luma with the updated worktree styles; verify their logs contain no CSS parser errors.
- [ ] Run `cargo test`, `cargo build --release`, `niri validate`, the repository shell syntax check, and `git diff --check`.
- [ ] Capture Luma and Waybar screenshots over the current wallpaper and confirm visible wallpaper bleed-through with readable text.
- [ ] Merge locally, regenerate live theme outputs, reload Hyprland and Waybar, and confirm `hyprctl configerrors` is empty.
