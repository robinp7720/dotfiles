# Vigil Morphing Transitions Design

## Summary

Replace Vigil's crossfades, slides, and fixed-duration enter/exit keyframes with
continuous geometry morphs on four surfaces: the center island, control-center
open/close, control-center page navigation, and notification toasts. All four
share one spring-driven morph engine written in Rust; no new dependencies.

## Morph Engine (`src/ui/morph.rs`)

The core is GTK-free and unit-tested:

- `Spring` integrates a damped spring at a fixed 120 Hz step. It exposes
  `retarget(target)`, which keeps current value and velocity so interrupted
  motion bends instead of restarting.
- Parameters derive from `MotionStyle`: `Bold` allows at most 4% overshoot and
  settles in about 380ms; `Calm` is critically damped (no overshoot) and settles
  in about 300ms. "Settled" means displacement and velocity are both below
  0.001 of the travel.
- `MorphShape { x, y, width, height, radius }` interpolates linearly by spring
  progress.
- `MorphTimeline` owns one spring and reports `progress()` and `is_settled()`.

## `MorphBin` Widget

A custom `gtk::Widget` subclass holding a single child.

- `morph_to(child, from: Option<graphene::Rect>)` snapshots the current child
  into a `gdk::Paintable` as the outgoing layer, installs the new child, and
  springs the shape from the previous shape (or `from`) to the new child's
  natural size and the CSS radius of the target.
- Snapshot rendering clips to the rounded shape. The outgoing paintable fades
  out over progress 0-0.4; the incoming child fades in over 0.2-1.0.
- Driven by `add_tick_callback`; the callback is removed once settled.
- Exposes `connect_settled` for owners that must act at rest (shrinking a
  layer surface, hiding the control-center window).
- When GTK animations are disabled (including the `reduced-motion` preview
  scenario) every morph completes synchronously.

## Center Island

The center `context_stack` becomes a `MorphBin`. Switching among title, media
(with controls), timer, activity, and suggestion contexts morphs the pill's
width and radius to the new content. The `context-arrived` CSS pulse is kept.
The suggestion-actions `Revealer` is removed; actions are part of the morphed
content.

## Control Center Open/Close

- Status buttons report their bounds in monitor coordinates with each open
  request. The panel starts as a pill exactly over that button, springs to the
  full panel, and on close springs back into the same button.
- The control-center window uses `exclusive_zone(-1)` and computes its own top
  margin (`SURFACE_MARGIN + BAR_HEIGHT + gap`) so it can overlap the bar.
- The layer surface is a transparent full-height stage anchored top/right/
  bottom, wide enough (set before each open) to contain both the origin and
  the panel, so no surface resizes happen during a morph or page change.
  `gdk::Surface::set_input_region` is reapplied on every frame-clock paint to
  restrict input to the current visible shape.
- Origin fallback order: requesting button bounds, then the right status
  cluster bounds, then a centered rectangle at 0.92 scale of the panel.
- Replaces `ControlCenterMotion`'s fixed 320/180ms timers and the
  `control-center-entering`/`-exiting` CSS keyframes; phase transitions are
  driven by the spring's settled signal.

## Control Center Pages

`NavigationUi::navigate` no longer uses slide transitions. Navigating from the
overview passes the activated tile's bounds (relative to the panel) as `from`,
so the tile expands into the detail page while panel height springs to the new
page's natural height. Back collapses the page into the tile it came from (or
the panel center when entered by IPC). Header title, subtitle, and footer text
crossfade.

## Notifications

- The toast stack moves to top-center, directly below the center island of the
  focused output, anchored top with computed margins and `exclusive_zone(-1)`.
- Rows are reconciled by notification id instead of rebuilt. An inserted toast
  morphs out of the island's rectangle; a removed toast collapses upward and
  fades; remaining rows spring their heights to reflow.
- The toast window is the same kind of full-height transparent stage
  (horizontally centered), with input limited to the visible cards. Outputs
  without a center island (reduced bars) use a top-center pill as the origin.
- Hover-pause, critical persistence, and DND behavior are unchanged.

## Edge Cases

- Retoggling mid-morph retargets the spring from its current shape.
- Output changes during a morph cancel it and snap to the end state.
- `vigil control-center close` (used by the session lock script) snaps closed
  without animation so nothing remains above the lock transition.
- Secondary outputs have no control center; only the island and toasts morph.

## Testing

Unit tests cover spring convergence and overshoot bounds for both styles,
velocity preservation on retarget, shape interpolation, origin fallback order,
keyed notification reconciliation (insert, remove, reorder), and toast/panel
margin computation. Verification also runs `cargo test`, `vigil preview`
including `reduced-motion`, mid-morph screenshots, and a live check on the
three-monitor Hyprland layout and Niri.
