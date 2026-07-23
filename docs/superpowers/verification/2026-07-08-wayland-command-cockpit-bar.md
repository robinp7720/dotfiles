# Wayland Command Cockpit Bar Verification

> Naming update (2026-07-23): this system was subsequently renamed **Vigil**.
> The identifiers below are preserved because they record the commands and
> observations from the original verification run.

Date: 2026-07-13
Session tested: Hyprland

## Automated Gate

Commands run from `/home/robin/.dotfiles/.worktrees/wayland-cockpit-bar`:

| Command | Result |
| --- | --- |
| `bash tools/bar/tests/desktop_contract.sh` | Pass, `desktop contract ok` |
| `bash -n setup.sh scripts/*.sh waybar/scripts/*.sh hypr/scripts/modes/*.sh bspwm/modes/*.sh bspwm/helpers/*` | Pass |
| `zsh -n zshrc bar/shell-integration.zsh` | Pass |
| `niri validate -c niri/config.kdl` | Pass, `config is valid` |
| `git diff --check` | Pass |
| `cargo fmt --manifest-path tools/bar/Cargo.toml --check` | Pass |
| `cargo clippy --manifest-path tools/bar/Cargo.toml --all-targets -- -D warnings` | Pass |
| `cargo test --manifest-path tools/bar/Cargo.toml` | Pass: 89 lib tests, 3 main tests, 7 calendar tests, 9 compositor tests, 0 failures |
| `cargo build --manifest-path tools/bar/Cargo.toml --release` | Pass |

Focused red-green defects found during live acceptance:

- `compositor_hyprland_clients_default_missing_urgent_to_false`: red on `missing field urgent`, green after defaulting omitted Hyprland client urgency to false.
- `bar_window_width_matches_monitor_width_minus_layer_margins`: added for the live compact-island surface defect; green after sizing layer windows to monitor width minus margins.

## Versions And Outputs

- Hyprland: `0.55.4`, commit `a0136d8c04687bb36eb8a28eb9d1ff92aea99704`.
- Niri binary: `26.04 (8ed0da4)`.
- Active session: `XDG_CURRENT_DESKTOP=Hyprland`, `WAYLAND_DISPLAY=wayland-1`.
- Niri live session was not available: `NIRI_SOCKET` was unset and `niri msg outputs` failed to connect.

Connected outputs during Hyprland acceptance:

- `DP-4`: active workspace 2, scale 1.00, reduced cockpit surface.
- `DP-5`: active workspace 3, focused, scale 1.00, primary cockpit surface.
- `HDMI-A-2`: active workspace 1, scale 1.00, reduced cockpit surface.

Layer geometry after fixes and with Waybar stopped:

- `DP-4`: `cockpit-bar` at `5 5 2150 46`.
- `DP-5`: `cockpit-bar` at `2165 680 3830 46`.
- `HDMI-A-2`: `cockpit-bar` at `6005 680 3830 46`.

## Hyprland Live Acceptance

Install and service smoke:

- Ran `./setup.sh`; it linked `bar/` to `~/.config/cockpit-bar` and linked the release binary to `~/.local/bin/cockpit-bar`.
- Ran `systemctl --user daemon-reload`.
- Ran `systemctl --user restart cockpit-bar.service`.
- Initial service start failed because current `hyprctl -j clients` omits `urgent`; fixed in `fix(bar): harden Hyprland live acceptance`.
- After rebuilding and restarting, `cockpit-bar.service` stayed `active (running)`.
- A post-start journal window from `2026-07-13T12:22:48+02:00` had no entries, confirming no reconnect loop or GTK warning spam after the cleanup fix.

Behavior exercised:

- Workspace/output context: confirmed via `hyprctl monitors`, `hyprctl activeworkspace -j`, and output-local layer placement.
- Window title context: visible on the primary and reduced bar screenshots; long active window title ellipsizes rather than shifting the layout.
- Keyboard layout: `hyprctl devices -j` reported the main keyboard using `English (US)`, displayed as `US` in the system cluster.
- Timer: ran `~/.local/bin/cockpit-bar timer start 5m --label 'Acceptance timer'`; `timer list` returned the active timer and the primary bar showed the final-five-minute timer state.
- Source workers: service process spawned `playerctl --follow --all-players` and `pactl subscribe`, and remained stable.

Visual evidence:

- `docs/superpowers/verification/screenshots/cockpit-hypr-dp5-primary-top.png`
- `docs/superpowers/verification/screenshots/cockpit-hypr-dp4-reduced-top.png`
- `docs/superpowers/verification/screenshots/cockpit-hypr-hdmi-a-2-reduced-top.png`

Observed visual status:

- Primary and reduced bars are full-width top bars after the width fix.
- Text remains readable over the current wallpaper.
- Reduced bars contain output-local workspace/title/clock only.
- Primary bar shows the system cluster and timer context.
- No popover overlap was observed in the captured states.

## Niri Acceptance

Static Niri validation passed with `niri validate -c niri/config.kdl`.

Live Niri checks were not performed in this run because the active session was Hyprland and `NIRI_SOCKET` was unset. The following remain deferred to a Niri login:

- Output-local workspace/window context under Niri.
- Niri keyboard layout switching through the bar.
- Primary-output restoration after temporary output removal.
- Niri screenshots for normal, urgent, calendar, timer, source-disconnected, and low-battery states.

## Unexercised Or Partially Exercised Scenarios

The following were not fully exercised live in this Hyprland pass:

- Calendar threshold with a real soon event.
- Build success/failure shell activity from the zsh hook.
- Network disconnect and optional-service disconnect/reconnect.
- Simulated low/critical discharging battery.
- Hardware output removal/reconnect.
- Manual click-through of every GTK popover and every Luma route.

These are covered by unit or policy tests where available, but still need manual live acceptance before removing the Waybar fallback entirely.

## Rollback

Immediate session rollback to Waybar:

```bash
systemctl --user stop cockpit-bar.service || true
systemctl --user reset-failed cockpit-bar.service || true
rm -f ~/.config/systemd/user/cockpit-bar.service
rm -f ~/.local/bin/cockpit-bar
rm -f ~/.config/cockpit-bar
systemctl --user daemon-reload
~/.config/waybar/launch.sh --replace
```

Tracked config rollback in this repository:

```bash
git checkout 2187ecb^ -- setup.sh hypr/hyprland-config/startup.conf hypr/hyprland-config/base.conf niri/config.kdl tools/bar/README.md
rm -f systemd/user/cockpit-bar.service tools/bar/tests/desktop_contract.sh
```

## Final Status

Accepted for automated gate and Hyprland smoke/visual placement after two live fixes:

- `fix(bar): harden Hyprland live acceptance`

Not accepted as complete cross-compositor live validation until the deferred Niri and scenario-specific checks above are run.
