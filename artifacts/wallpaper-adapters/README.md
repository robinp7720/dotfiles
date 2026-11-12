# Wallpaper adapter verification

Validated on 2026-11-12 in an isolated Niri session nested in headless Weston.
Screenshots use Luma's default styling, fixture display labels, and local sample
images. No physical monitor or user wallpaper choices were changed.

- `picker.png`: keyboard-selected Main display with desktop color updates disabled.
  Applying emitted exactly one backend request targeting `right`, with no theme call.
- `theme-failure.png`: backend success followed by a failing theme helper. The UI
  reports that the wallpaper applied and the theme update failed separately.
- The real dotfiles adapter discovered Niri's `winit` output, applied an image using
  a real isolated awww daemon, and persisted that output's choice. `awww query`
  confirmed the image. The systemctl restart was stubbed for this isolated check.
- Luma: 185 tests passed, one existing test ignored; release build passed with
  existing mail protocol dead-code warnings.
- Adapter unit tests and `tests/wallpaper_adapter_test.sh` passed. The integration
  test covers explicit targets, shell metacharacters, persistence, failures,
  independent theme generation, Niri filtering, and additive setup configuration.
- Wallpaper watcher, setup build/failure isolation, and desktop theme contracts
  passed. The full Vigil desktop contract also passed after updating its setup
  fixtures for the new adapter. Shell syntax and Rust formatting checks passed.

The installed wallpaper services were inactive in this environment. End-to-end
systemd application on physical Hyprland/Niri displays remains a live-session check.
