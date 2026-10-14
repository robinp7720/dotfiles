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
- Parameters derive from `MotionStyle`: `Bold` (damping 0.70, 18 rad/s) overshoots
  about 3% and settles in about 430ms; `Calm` (critically damped, 22 rad/s) never
  overshoots and settles in about 440ms. Frame gaps are clamped to 50ms. Progress
  springs settle below 0.002 displacement and 0.05/s velocity; pixel springs below
  0.5px and 5px/s.
- `MorphRect { x, y, width, height }` interpolates by spring progress without
  clamping, so overshoot carries through, while sizes never go negative.
- A thread-local motion style, set from each bar render, selects the parameters.

## `MorphBin` Widget (`src/ui/morph_bin.rs`)

A custom `gtk::Widget` subclass holding a single child.

- `set_follow(width, height)` springs the bin's measured size toward the
  child's natural size (heights are measured at the allocated width); content
  is clipped while resizing.
- `crossfade(mutate)` captures the current content as a static paintable, runs
  `mutate`, and fades the snapshot out (0-0.4) while the live child fades in
  (0.2-1.0).
- `expand_from(origin, capture, mutate)` and `collapse_into(target, capture,
  mutate)` are container transforms between an origin rectangle (root-window
  coordinates, a centered scale, or local) and the bin's bounds. Content is
  scaled into a rounded clip whose radius grows from the origin's pill radius;
  an optional outset shadow matching the CSS shadow is drawn while moving.
- Interrupting any transform continues from the shape currently on screen,
  with velocity carried over.
- `on_settled(callback)` runs once motion rests (immediately if idle); any
  later transition or `cancel()` drops it.
- Driven by `add_tick_callback`, which stops when everything settles. When GTK
  animations are disabled every transition completes synchronously.

## Center Island

The center `context_stack` becomes a `MorphBin`. Content crossfades only when
its identity changes (title vs. context, card kind, suggestion); countdown and
clock text updates apply in place while the width follows. Switching among title, media
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
  restrict input to the visible shape clipped to the panel's resting bounds,
  so the stage never takes input over the bar. Because Hyprland gives pointer
  focus to a newly mapped overlay under a still pointer, a press on the stage
  outside the panel also dismisses it.
- Origin fallback order: requesting button bounds, then the right status
  cluster bounds, then a centered rectangle at 0.92 scale of the panel.
- Replaces `ControlCenterMotion`'s fixed 320/180ms timers and the
  `control-center-entering`/`-exiting` CSS keyframes; phase transitions are
  driven by the spring's settled signal.

## Control Center Pages

`NavigationUi::navigate` no longer uses slide transitions. Navigating from the
overview passes the activated tile's bounds (relative to the panel) as `from`,
so the tile expands into the detail page while panel height springs to the new
page's natural height. Back collapses the page into the tile it came from;
pages entered without a tile (status items, IPC) crossfade instead. The header
crossfades when the page changes.

## Notifications

- The toast stack moves to top-center, directly below the center island of the
  focused output, anchored top with computed margins and `exclusive_zone(-1)`.
- Rows are reconciled by notification id instead of rebuilt. An inserted toast
  morphs out of the island's rectangle; a removed toast collapses upward and
  fades; remaining rows spring their heights to reflow.
- The toast window is the same kind of full-height transparent stage
  (horizontally centered), with input limited to the visible cards below the
  bar, and Hyprland's layer animation disabled for its namespace. Outputs
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
