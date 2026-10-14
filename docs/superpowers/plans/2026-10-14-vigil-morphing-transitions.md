# Vigil Morphing Transitions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Vigil's crossfades, slides, and fixed keyframe enter/exit with spring-driven geometry morphs on the center island, control-center open/close, control-center pages, and notification toasts.

**Architecture:** A GTK-free core (`ui/morph.rs`: spring, rect math, opacity curves, motion-style global) feeds a single custom widget (`ui/morph_bin.rs`: `MorphBin`) that animates its own size, clips, crossfades captured snapshots, and performs container transforms from/into rectangles. Overlay layer surfaces become fixed-size transparent "stages" (`ui/stage.rs`) that ignore exclusive zones, so morphs can start over bar items without per-frame surface resizes; input is restricted to the visible shapes with `gdk::Surface::set_input_region`.

**Tech Stack:** Rust 2024, gtk4-rs 0.10 (GTK 4.24 runtime), gtk4-layer-shell 0.7, GSK snapshot API.

**Spec:** `docs/superpowers/specs/2026-10-14-vigil-morphing-transitions-design.md`

## Global Constraints

- No new crate dependencies.
- Spring parameters: `Bold` = damping ratio 0.70, angular frequency 18.0 rad/s (≈3.1% overshoot, settles ≈0.43s); `Calm` = damping ratio 1.0, angular frequency 22.0 rad/s (no overshoot, settles ≈0.44s). Fixed integration step 1/120 s; frame deltas clamped to 0.05 s.
- Overshoot for `Bold` must stay ≤ 4%.
- When `gtk::Settings::is_gtk_enable_animations()` is false (including `vigil preview reduced-motion`), every morph completes synchronously.
- `vigil control-center close` must hide the panel immediately without animation (used by `scripts/session_lock.sh`).
- Layer namespaces stay `vigil`, `vigil-control-center`, `vigil-notifications`.
- Shell/CSS/Rust style per `AGENTS.md`; `cargo fmt`; Conventional Commits with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `tools/bar` is the Vigil git submodule. Rust/README changes are committed inside `tools/bar` on its branch `feat/vigil-morphing-transitions`; `bar/style.css`, `tests/`, and `docs/` are committed in the dotfiles root repo on its branch `feat/vigil-morphing-transitions`. A task touching both makes one commit in each repo. Commit trailers use the Co-Authored-By line from your own harness attribution reminder.
- All cargo commands run from `tools/bar`. CSS lives in `bar/style.css` (symlinked to `~/.config/vigil/style.css`).

## File Map

| File | Responsibility |
|------|----------------|
| `tools/bar/src/ui/morph.rs` (new) | Spring, `MorphRect`, opacity curves, content scale, motion-style thread-local. Pure, unit-tested. |
| `tools/bar/src/ui/stage.rs` (new) | Bar/stage geometry constants and coordinate conversions (pure, tested) plus `track_input` (GTK input-region upkeep). |
| `tools/bar/src/ui/morph_bin.rs` (new) | `MorphBin` widget: size following, crossfade, expand/collapse transforms, settled callbacks. |
| `tools/bar/src/ui/mod.rs` | Register new modules. |
| `tools/bar/src/ui/surface.rs` | Island uses `MorphBin`; status buttons/context pass origin rects; toast stack moves top-center with keyed rows; constants move to `stage.rs`. |
| `tools/bar/src/ui/control_center.rs` | Stage window, panel `MorphBin`, `ControlCenterMotion` driven by springs, page `MorphBin` navigation. |
| `bar/style.css` | Drop control-center and toast keyframe enter/exit. |
| `tests/vigil_desktop_contract.sh` | Update contract strings. |
| `tools/bar/README.md` | Document motion and new toast placement. |

---

### Task 1: Morph core (`morph.rs`)

**Files:**
- Create: `tools/bar/src/ui/morph.rs`
- Modify: `tools/bar/src/ui/mod.rs:1-9` (module list)

**Interfaces:**
- Produces:
  - `pub struct SpringParams { pub damping_ratio: f64, pub angular_frequency: f64 }`, `SpringParams::for_style(MotionStyle) -> SpringParams`
  - `pub struct Spring` with `Spring::progress(SpringParams) -> Spring` (value 0, target 0, eps 0.002 / 0.05), `Spring::pixels(SpringParams, f64) -> Spring` (eps 0.5 / 5.0), `value()`, `velocity()`, `target()`, `retarget(f64)`, `restart(from: f64, to: f64, velocity: f64)`, `snap(f64)`, `set_params(SpringParams)`, `advance(seconds: f64)`, `is_settled() -> bool`
  - `pub struct MorphRect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }` with `new`, `lerp(self, to, t: f32)`, `translated(dx, dy)`, `scaled_about_center(f32)`, `union(self, other)`
  - `pub fn fade_in(progress: f64, start: f64, end: f64) -> f64`, `pub fn fade_out(progress: f64, start: f64, end: f64) -> f64`
  - `pub fn content_scale(shape: MorphRect, full: MorphRect) -> f32`
  - `pub fn set_motion_style(MotionStyle)`, `pub fn motion_style() -> MotionStyle`

- [ ] **Step 1: Register the module and write failing tests**

In `tools/bar/src/ui/mod.rs` add `mod morph;` after `mod artwork;` (keep alphabetical order of the private block):

```rust
mod ai_usage;
mod artwork;
mod morph;
pub mod context_card;
```

Create `tools/bar/src/ui/morph.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::MotionStyle;

    fn run_to_rest(params: SpringParams) -> (f64, f64) {
        let mut spring = Spring::progress(params);
        spring.retarget(1.0);
        let mut peak = 0.0_f64;
        let mut elapsed = 0.0;
        while !spring.is_settled() && elapsed < 2.0 {
            spring.advance(1.0 / 60.0);
            elapsed += 1.0 / 60.0;
            peak = peak.max(spring.value());
        }
        (peak, elapsed)
    }

    #[test]
    fn bold_spring_overshoots_slightly_and_settles() {
        let (peak, elapsed) = run_to_rest(SpringParams::for_style(MotionStyle::Bold));
        assert!(peak > 1.02 && peak <= 1.04, "peak {peak}");
        assert!(elapsed <= 0.5, "elapsed {elapsed}");
    }

    #[test]
    fn calm_spring_never_overshoots_and_settles() {
        let (peak, elapsed) = run_to_rest(SpringParams::for_style(MotionStyle::Calm));
        assert!(peak <= 1.0, "peak {peak}");
        assert!(elapsed <= 0.5, "elapsed {elapsed}");
    }

    #[test]
    fn settled_spring_lands_exactly_on_target() {
        let mut spring = Spring::pixels(SpringParams::for_style(MotionStyle::Bold), 10.0);
        spring.retarget(200.0);
        for _ in 0..120 {
            spring.advance(1.0 / 60.0);
        }
        assert!(spring.is_settled());
        assert_eq!(spring.value(), 200.0);
        assert_eq!(spring.velocity(), 0.0);
    }

    #[test]
    fn retarget_preserves_velocity() {
        let mut spring = Spring::progress(SpringParams::for_style(MotionStyle::Bold));
        spring.retarget(1.0);
        spring.advance(0.05);
        let velocity = spring.velocity();
        assert!(velocity > 0.0);
        spring.retarget(0.0);
        assert_eq!(spring.velocity(), velocity);
    }

    #[test]
    fn large_frame_gaps_are_clamped() {
        let mut spring = Spring::progress(SpringParams::for_style(MotionStyle::Bold));
        spring.retarget(1.0);
        spring.advance(10.0);
        assert!(spring.value() < 1.0, "a stalled frame must not jump to rest");
    }

    #[test]
    fn rect_lerp_extrapolates_but_never_inverts_size() {
        let from = MorphRect::new(0.0, 0.0, 10.0, 10.0);
        let to = MorphRect::new(100.0, 50.0, 110.0, 60.0);
        assert_eq!(from.lerp(to, 0.5), MorphRect::new(50.0, 25.0, 60.0, 35.0));
        assert_eq!(from.lerp(to, 1.1).x, 110.0);
        assert_eq!(to.lerp(from, 2.5).width, 0.0);
    }

    #[test]
    fn rect_helpers_translate_scale_and_union() {
        let rect = MorphRect::new(10.0, 20.0, 100.0, 40.0);
        assert_eq!(rect.translated(-10.0, 5.0), MorphRect::new(0.0, 25.0, 100.0, 40.0));
        assert_eq!(rect.scaled_about_center(0.5), MorphRect::new(35.0, 30.0, 50.0, 20.0));
        assert_eq!(
            rect.union(MorphRect::new(0.0, 0.0, 20.0, 20.0)),
            MorphRect::new(0.0, 0.0, 110.0, 60.0)
        );
    }

    #[test]
    fn fades_are_clamped_ramps() {
        assert_eq!(fade_in(-0.2, 0.0, 0.4), 0.0);
        assert_eq!(fade_in(0.2, 0.0, 0.4), 0.5);
        assert_eq!(fade_in(1.2, 0.0, 0.4), 1.0);
        assert_eq!(fade_out(0.2, 0.0, 0.4), 0.5);
    }

    #[test]
    fn content_scale_follows_width_ratio() {
        let full = MorphRect::new(0.0, 0.0, 480.0, 600.0);
        assert_eq!(content_scale(MorphRect::new(0.0, 0.0, 240.0, 10.0), full), 0.5);
        assert_eq!(content_scale(full, MorphRect::default()), 1.0);
    }

    #[test]
    fn motion_style_defaults_to_bold_and_can_change() {
        assert_eq!(motion_style(), MotionStyle::Bold);
        set_motion_style(MotionStyle::Calm);
        assert_eq!(motion_style(), MotionStyle::Calm);
        set_motion_style(MotionStyle::Bold);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::morph`
Expected: compile errors (`cannot find type SpringParams`, etc.).

- [ ] **Step 3: Implement the core above the test module**

```rust
//! Spring-driven geometry shared by Vigil's morphing surfaces. GTK-free.

use std::cell::Cell;

use crate::MotionStyle;

const STEP_SECONDS: f64 = 1.0 / 120.0;
const MAX_FRAME_SECONDS: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringParams {
    pub damping_ratio: f64,
    pub angular_frequency: f64,
}

impl SpringParams {
    pub fn for_style(style: MotionStyle) -> Self {
        match style {
            MotionStyle::Bold => Self {
                damping_ratio: 0.70,
                angular_frequency: 18.0,
            },
            MotionStyle::Calm => Self {
                damping_ratio: 1.0,
                angular_frequency: 22.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    params: SpringParams,
    value: f64,
    velocity: f64,
    target: f64,
    position_epsilon: f64,
    velocity_epsilon: f64,
    remainder: f64,
}

impl Spring {
    fn new(params: SpringParams, value: f64, position_epsilon: f64, velocity_epsilon: f64) -> Self {
        Self {
            params,
            value,
            velocity: 0.0,
            target: value,
            position_epsilon,
            velocity_epsilon,
            remainder: 0.0,
        }
    }

    /// A normalized 0..1 progress spring.
    pub fn progress(params: SpringParams) -> Self {
        Self::new(params, 0.0, 0.002, 0.05)
    }

    /// A spring measured in logical pixels.
    pub fn pixels(params: SpringParams, value: f64) -> Self {
        Self::new(params, value, 0.5, 5.0)
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn velocity(&self) -> f64 {
        self.velocity
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    pub fn retarget(&mut self, target: f64) {
        self.target = target;
    }

    pub fn restart(&mut self, from: f64, to: f64, velocity: f64) {
        self.value = from;
        self.target = to;
        self.velocity = velocity;
        self.remainder = 0.0;
    }

    pub fn snap(&mut self, value: f64) {
        self.restart(value, value, 0.0);
    }

    pub fn set_params(&mut self, params: SpringParams) {
        self.params = params;
    }

    pub fn advance(&mut self, seconds: f64) {
        let mut remaining = self.remainder + seconds.clamp(0.0, MAX_FRAME_SECONDS);
        while remaining >= STEP_SECONDS {
            self.step(STEP_SECONDS);
            remaining -= STEP_SECONDS;
        }
        self.remainder = remaining;
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < self.position_epsilon
            && self.velocity.abs() < self.velocity_epsilon
    }

    fn step(&mut self, seconds: f64) {
        let omega = self.params.angular_frequency;
        let stiffness = omega * omega;
        let damping = 2.0 * self.params.damping_ratio * omega;
        let acceleration = -stiffness * (self.value - self.target) - damping * self.velocity;
        self.velocity += acceleration * seconds;
        self.value += self.velocity * seconds;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MorphRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl MorphRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Unclamped interpolation so spring overshoot carries through; sizes never go negative.
    pub fn lerp(self, to: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            x: mix(self.x, to.x),
            y: mix(self.y, to.y),
            width: mix(self.width, to.width).max(0.0),
            height: mix(self.height, to.height).max(0.0),
        }
    }

    pub fn translated(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    pub fn scaled_about_center(self, factor: f32) -> Self {
        let width = self.width * factor;
        let height = self.height * factor;
        Self {
            x: self.x + (self.width - width) / 2.0,
            y: self.y + (self.height - height) / 2.0,
            width,
            height,
        }
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self::new(x, y, right - x, bottom - y)
    }
}

pub fn fade_in(progress: f64, start: f64, end: f64) -> f64 {
    ((progress - start) / (end - start)).clamp(0.0, 1.0)
}

pub fn fade_out(progress: f64, start: f64, end: f64) -> f64 {
    1.0 - fade_in(progress, start, end)
}

/// Uniform scale that fits full-size content to the morph shape's width.
pub fn content_scale(shape: MorphRect, full: MorphRect) -> f32 {
    if full.width <= 0.0 {
        1.0
    } else {
        (shape.width / full.width).max(0.0)
    }
}

thread_local! {
    static MOTION_STYLE: Cell<MotionStyle> = const { Cell::new(MotionStyle::Bold) };
}

pub fn set_motion_style(style: MotionStyle) {
    MOTION_STYLE.with(|cell| cell.set(style));
}

pub fn motion_style() -> MotionStyle {
    MOTION_STYLE.with(Cell::get)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ui::morph`
Expected: 10 tests pass. If `bold_spring_overshoots_slightly_and_settles` reports a peak outside (1.02, 1.04], do not loosen the test; re-check the integrator matches the semi-implicit Euler above (velocity updated before position).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/ui/morph.rs src/ui/mod.rs
git commit -m "feat(vigil): add spring morph core

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Stage geometry and input regions (`stage.rs`)

**Files:**
- Create: `tools/bar/src/ui/stage.rs`
- Modify: `tools/bar/src/ui/mod.rs` (add `mod stage;`)
- Modify: `tools/bar/src/ui/surface.rs:32-34` (constants move to `stage.rs`)

**Interfaces:**
- Consumes: `MorphRect` from Task 1.
- Produces:
  - `pub const BAR_HEIGHT: i32 = 44; pub const SURFACE_MARGIN: i32 = 5; pub const NOTIFICATION_GAP: i32 = 10; pub const CONTROL_CENTER_RADIUS: f32 = 18.0; pub const CARD_RADIUS: f32 = 14.0;`
  - `pub fn bar_to_monitor(rect: MorphRect) -> MorphRect`
  - `pub fn panel_top_in_stage() -> i32` (= 49)
  - `pub fn toast_top_in_stage() -> i32` (= 54)
  - `pub fn control_center_stage_width(panel_width: i32, origin: Option<MorphRect>, monitor_width: i32) -> i32`
  - `pub fn monitor_to_top_right_stage(rect: MorphRect, monitor_width: i32, stage_width: i32) -> MorphRect`
  - `pub fn toast_origin(island_on_monitor: Option<MorphRect>, monitor_width: i32, stage_width: i32) -> MorphRect`
  - `pub fn track_input(window: &gtk::ApplicationWindow, shapes: impl Fn() -> Vec<MorphRect> + 'static)`

Geometry background: the bar window is anchored top/left/right with `SURFACE_MARGIN` margins and reserves a `BAR_HEIGHT` exclusive zone, so the work area starts at y = 49 and the old panel sat at y = 54 (work area + `SURFACE_MARGIN`); toasts sat at y = 59 (work area + `NOTIFICATION_GAP`). Stage windows use `exclusive_zone(-1)` and a top margin of `SURFACE_MARGIN`, so their window y = 0 is monitor y = 5. Keeping the panel at monitor y = 54 means a 49 px offset in the stage; toasts at monitor y = 59 means 54 px.

- [ ] **Step 1: Write the failing tests**

Create `tools/bar/src/ui/stage.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_rects_are_offset_by_the_layer_margins() {
        assert_eq!(
            bar_to_monitor(MorphRect::new(100.0, 4.0, 30.0, 28.0)),
            MorphRect::new(105.0, 9.0, 30.0, 28.0)
        );
    }

    #[test]
    fn stage_offsets_keep_previous_absolute_positions() {
        assert_eq!(SURFACE_MARGIN + panel_top_in_stage(), 54);
        assert_eq!(SURFACE_MARGIN + toast_top_in_stage(), 59);
    }

    #[test]
    fn control_center_stage_widens_to_reach_the_origin() {
        assert_eq!(control_center_stage_width(512, None, 2560), 512);
        let near = MorphRect::new(2300.0, 9.0, 30.0, 28.0);
        assert_eq!(control_center_stage_width(512, Some(near), 2560), 512);
        let far = MorphRect::new(1900.0, 9.0, 30.0, 28.0);
        assert_eq!(control_center_stage_width(512, Some(far), 2560), 655);
        let beyond = MorphRect::new(-50.0, 9.0, 30.0, 28.0);
        assert_eq!(control_center_stage_width(512, Some(beyond), 1000), 990);
    }

    #[test]
    fn monitor_rects_map_into_a_top_right_stage() {
        let rect = MorphRect::new(2300.0, 9.0, 30.0, 28.0);
        assert_eq!(
            monitor_to_top_right_stage(rect, 2560, 512),
            MorphRect::new(257.0, 4.0, 30.0, 28.0)
        );
    }

    #[test]
    fn toasts_grow_from_the_island_or_a_top_center_pill() {
        let island = MorphRect::new(1080.0, 9.0, 400.0, 36.0);
        assert_eq!(
            toast_origin(Some(island), 2560, 400),
            MorphRect::new(0.0, 4.0, 400.0, 36.0)
        );
        assert_eq!(
            toast_origin(None, 1920, 400),
            MorphRect::new(120.0, 4.0, 160.0, 36.0)
        );
    }
}
```

Add `mod stage;` to `tools/bar/src/ui/mod.rs` after `mod morph;`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::stage`
Expected: compile errors for missing functions.

- [ ] **Step 3: Implement**

Prepend to `stage.rs`:

```rust
//! Geometry for transparent overlay "stages" that host morphing surfaces.
//!
//! Stage windows ignore exclusive zones and sit at `SURFACE_MARGIN` from the top edge, so
//! a morph can start over a bar item without resizing the layer surface every frame.

use gtk4::{self as gtk, cairo, prelude::*};

use super::morph::MorphRect;

pub const BAR_HEIGHT: i32 = 44;
pub const SURFACE_MARGIN: i32 = 5;
pub const NOTIFICATION_GAP: i32 = 10;
pub const CONTROL_CENTER_RADIUS: f32 = 18.0;
pub const CARD_RADIUS: f32 = 14.0;

const FALLBACK_TOAST_ORIGIN_WIDTH: f32 = 160.0;
const FALLBACK_TOAST_ORIGIN_HEIGHT: f32 = 36.0;

/// The bar window is anchored top/left/right at `SURFACE_MARGIN`.
pub fn bar_to_monitor(rect: MorphRect) -> MorphRect {
    rect.translated(SURFACE_MARGIN as f32, SURFACE_MARGIN as f32)
}

pub fn panel_top_in_stage() -> i32 {
    BAR_HEIGHT + SURFACE_MARGIN
}

pub fn toast_top_in_stage() -> i32 {
    BAR_HEIGHT + NOTIFICATION_GAP
}

pub fn control_center_stage_width(
    panel_width: i32,
    origin: Option<MorphRect>,
    monitor_width: i32,
) -> i32 {
    let available = (monitor_width - SURFACE_MARGIN * 2).max(1);
    let reach = origin.map_or(panel_width, |origin| {
        panel_width.max(monitor_width - SURFACE_MARGIN - origin.x.floor() as i32)
    });
    reach.clamp(1, available)
}

/// A stage anchored top-right with `SURFACE_MARGIN` margins.
pub fn monitor_to_top_right_stage(
    rect: MorphRect,
    monitor_width: i32,
    stage_width: i32,
) -> MorphRect {
    let left = monitor_width - SURFACE_MARGIN - stage_width;
    rect.translated(-left as f32, -SURFACE_MARGIN as f32)
}

/// A horizontally centered stage with a `SURFACE_MARGIN` top margin.
pub fn toast_origin(
    island_on_monitor: Option<MorphRect>,
    monitor_width: i32,
    stage_width: i32,
) -> MorphRect {
    let island = island_on_monitor.unwrap_or_else(|| {
        MorphRect::new(
            (monitor_width as f32 - FALLBACK_TOAST_ORIGIN_WIDTH) / 2.0,
            SURFACE_MARGIN as f32 + 4.0,
            FALLBACK_TOAST_ORIGIN_WIDTH,
            FALLBACK_TOAST_ORIGIN_HEIGHT,
        )
    });
    let left = (monitor_width - stage_width) / 2;
    island.translated(-left as f32, -SURFACE_MARGIN as f32)
}

/// Keeps the window's input region equal to the given window-coordinate shapes.
///
/// Applied during every frame-clock paint so the region lands in the same commit as the
/// pixels, and GTK's own allocation-time resets are overwritten.
pub fn track_input(window: &gtk::ApplicationWindow, shapes: impl Fn() -> Vec<MorphRect> + 'static) {
    let shapes = std::rc::Rc::new(shapes);
    window.connect_realize(move |window| {
        let Some(clock) = window.frame_clock() else {
            return;
        };
        let window = window.downgrade();
        let shapes = shapes.clone();
        clock.connect_paint(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let Some(surface) = window.surface() else {
                return;
            };
            let region = cairo::Region::create();
            for shape in shapes() {
                let rectangle = cairo::RectangleInt::new(
                    shape.x.floor() as i32,
                    shape.y.floor() as i32,
                    shape.width.ceil() as i32 + 1,
                    shape.height.ceil() as i32 + 1,
                );
                let _ = region.union_rectangle(&rectangle);
            }
            surface.set_input_region(&region);
        });
    });
}
```

In `tools/bar/src/ui/surface.rs` delete lines 32-34 (`const BAR_HEIGHT`, `const SURFACE_MARGIN`, `const NOTIFICATION_GAP`) and add to the `use super::...` imports:

```rust
use super::stage::{BAR_HEIGHT, NOTIFICATION_GAP, SURFACE_MARGIN};
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib ui::stage && cargo build`
Expected: 5 stage tests pass; build succeeds (warnings for unused functions are expected until later tasks; do not silence them with `allow`).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/ui/stage.rs src/ui/mod.rs src/ui/surface.rs
git commit -m "feat(vigil): add overlay stage geometry and input regions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `MorphBin` widget

**Files:**
- Create: `tools/bar/src/ui/morph_bin.rs`
- Modify: `tools/bar/src/ui/mod.rs` (add `mod morph_bin;`)

**Interfaces:**
- Consumes: `Spring`, `SpringParams`, `MorphRect`, `fade_in`, `fade_out`, `content_scale`, `motion_style` (Task 1).
- Produces:
  - `pub enum MorphOrigin { Window(MorphRect), Scaled(f32), Local(MorphRect) }` — `Window` is in the coordinates of the bin's root window; `Scaled` scales the bin's own bounds about their center; `Local` is in the bin's own coordinates.
  - `pub struct MorphShadow { pub dy: f32, pub blur: f32, pub alpha: f32 }`
  - `MorphBin::new()`, `set_child(Option<&impl IsA<gtk::Widget>>)`, `child() -> Option<gtk::Widget>`, `set_follow(width: bool, height: bool)`, `set_start_collapsed(bool)`, `set_radius(f32)`, `set_shadow(Option<MorphShadow>)`
  - `crossfade(&self, mutate: impl FnOnce())`
  - `expand_from(&self, origin: MorphOrigin, capture: bool, mutate: impl FnOnce())` — live child grows from `origin`; with `capture`, the pre-mutation content stays underneath and fades out.
  - `collapse_into(&self, target: MorphOrigin, capture: bool, mutate: impl FnOnce())` — without `capture` the live child shrinks into `target`; with `capture` a snapshot of the pre-mutation content shrinks into `target` while the mutated live child fades in underneath.
  - `on_settled(&self, callback: impl FnOnce() + 'static)` — runs once when all motion rests; runs immediately when idle; replaced (dropped uncalled) by any later transition or `cancel`.
  - `cancel(&self)` — stops motion, drops the pending callback, snaps sizes.
  - `is_animating() -> bool`
  - `shape_in(&self, ancestor: &impl IsA<gtk::Widget>) -> Option<MorphRect>` — current visible shape in ancestor coordinates.
  - `pub fn animations_enabled() -> bool`

The widget is GTK-bound; its math lives in Task 1 and is already tested. Verification here is a build plus a visual smoke test in Task 4.

- [ ] **Step 1: Create the widget**

Add `mod morph_bin;` after `mod morph;` in `tools/bar/src/ui/mod.rs`. Create `tools/bar/src/ui/morph_bin.rs`:

```rust
//! A single-child container whose geometry morphs on springs.

use std::cell::{Cell, RefCell};

use gtk4::{self as gtk, gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

use super::morph::{
    self, MorphRect, Spring, SpringParams, content_scale, fade_in, fade_out,
};
use crate::MotionStyle;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MorphOrigin {
    Window(MorphRect),
    Scaled(f32),
    Local(MorphRect),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphShadow {
    pub dy: f32,
    pub blur: f32,
    pub alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransformKind {
    Expand,
    Collapse,
}

struct Transform {
    kind: TransformKind,
    origin: MorphOrigin,
    resolved: Option<MorphRect>,
    capture: bool,
    progress: Spring,
}

struct Ghost {
    paintable: gdk::Paintable,
    rect: MorphRect,
}

pub fn animations_enabled() -> bool {
    gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

fn params() -> SpringParams {
    SpringParams::for_style(morph::motion_style())
}

fn to_graphene(rect: MorphRect) -> graphene::Rect {
    graphene::Rect::new(rect.x, rect.y, rect.width, rect.height)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MorphBin {
        pub(super) child: RefCell<Option<gtk::Widget>>,
        pub(super) follow_width: Cell<bool>,
        pub(super) follow_height: Cell<bool>,
        pub(super) start_collapsed: Cell<bool>,
        pub(super) width: RefCell<Option<Spring>>,
        pub(super) height: RefCell<Option<Spring>>,
        pub(super) radius: Cell<f32>,
        pub(super) shadow: Cell<Option<MorphShadow>>,
        pub(super) transform: RefCell<Option<Transform>>,
        pub(super) ghost: RefCell<Option<Ghost>>,
        pub(super) crossfade: RefCell<Option<Spring>>,
        pub(super) tick: RefCell<Option<gtk::TickCallbackId>>,
        pub(super) last_frame_us: Cell<Option<i64>>,
        pub(super) settled: RefCell<Option<Box<dyn FnOnce()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MorphBin {
        const NAME: &'static str = "VigilMorphBin";
        type Type = super::MorphBin;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for MorphBin {
        fn dispose(&self) {
            if let Some(tick) = self.tick.take() {
                tick.remove();
            }
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MorphBin {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.child.borrow().clone() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, for_size);
            let horizontal = orientation == gtk::Orientation::Horizontal;
            let follows = if horizontal {
                self.follow_width.get()
            } else {
                self.follow_height.get()
            };
            if !follows {
                return (minimum, natural, -1, -1);
            }
            let tracks = horizontal || for_size < 0 || for_size == self.obj().width();
            let slot = if horizontal { &self.width } else { &self.height };
            let target = f64::from(natural);
            let mut start_ticking = false;
            {
                let mut spring = slot.borrow_mut();
                match spring.as_mut() {
                    None => {
                        let collapsed = self.start_collapsed.get() && animations_enabled();
                        let mut created =
                            Spring::pixels(params(), if collapsed { 0.0 } else { target });
                        created.retarget(target);
                        start_ticking = collapsed;
                        *spring = Some(created);
                    }
                    Some(existing) if tracks && (existing.target() - target).abs() >= 0.5 => {
                        if animations_enabled() && self.obj().is_mapped() {
                            existing.retarget(target);
                            start_ticking = true;
                        } else {
                            existing.snap(target);
                        }
                    }
                    Some(_) => {}
                }
            }
            if start_ticking {
                self.ensure_ticking();
            }
            let current = slot
                .borrow()
                .as_ref()
                .map_or(natural, |spring| spring.value().round().max(0.0) as i32);
            (minimum.min(current), current, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            let (child_width, x) = if self.follow_width.get() {
                let (minimum, natural, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
                let child_width = natural.max(width).max(minimum);
                (child_width, (width - child_width) / 2)
            } else {
                (width, 0)
            };
            let child_height = if self.follow_height.get() {
                let (minimum, natural, _, _) =
                    child.measure(gtk::Orientation::Vertical, child_width);
                natural.max(height).max(minimum)
            } else {
                height
            };
            let transform =
                gsk::Transform::new().translate(&graphene::Point::new(x as f32, 0.0));
            child.allocate(child_width, child_height, baseline, Some(transform));
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            if self.child.borrow().is_none() {
                return;
            }
            let bounds = MorphRect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32);
            self.resolve_origin(bounds);
            let resizing = self.is_resizing();
            if resizing {
                snapshot.push_clip(&to_graphene(bounds));
            }
            match self.transform.borrow().as_ref() {
                None => self.snapshot_crossfade(snapshot, bounds),
                Some(transform) => self.snapshot_transform(snapshot, bounds, transform),
            }
            if resizing {
                snapshot.pop();
            }
        }
    }

    impl MorphBin {
        pub(super) fn is_resizing(&self) -> bool {
            [&self.width, &self.height]
                .into_iter()
                .any(|slot| slot.borrow().as_ref().is_some_and(|spring| !spring.is_settled()))
        }

        pub(super) fn is_animating(&self) -> bool {
            self.is_resizing()
                || self
                    .transform
                    .borrow()
                    .as_ref()
                    .is_some_and(|transform| !transform.progress.is_settled())
                || self
                    .crossfade
                    .borrow()
                    .as_ref()
                    .is_some_and(|spring| !spring.is_settled())
        }

        pub(super) fn ensure_ticking(&self) {
            if self.tick.borrow().is_some() {
                return;
            }
            self.last_frame_us.set(None);
            let id = self.obj().add_tick_callback(|obj, clock| {
                let imp = obj.imp();
                let now = clock.frame_time();
                let seconds = imp
                    .last_frame_us
                    .replace(Some(now))
                    .map_or(0.0, |last| (now - last) as f64 / 1_000_000.0);
                imp.advance(seconds);
                if imp.is_animating() {
                    obj.queue_resize();
                    glib::ControlFlow::Continue
                } else {
                    imp.tick.take();
                    imp.complete();
                    obj.queue_resize();
                    glib::ControlFlow::Break
                }
            });
            *self.tick.borrow_mut() = Some(id);
        }

        fn advance(&self, seconds: f64) {
            let params = params();
            for slot in [&self.width, &self.height] {
                if let Some(spring) = slot.borrow_mut().as_mut() {
                    spring.set_params(params);
                    spring.advance(seconds);
                }
            }
            if let Some(transform) = self.transform.borrow_mut().as_mut() {
                transform.progress.set_params(params);
                transform.progress.advance(seconds);
            }
            if let Some(spring) = self.crossfade.borrow_mut().as_mut() {
                spring.advance(seconds);
            }
        }

        pub(super) fn complete(&self) {
            self.transform.take();
            self.ghost.take();
            self.crossfade.take();
            self.obj().queue_draw();
            if let Some(callback) = self.settled.take() {
                callback();
            }
        }

        pub(super) fn bounds(&self) -> MorphRect {
            let obj = self.obj();
            MorphRect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32)
        }

        pub(super) fn resolve_origin(&self, bounds: MorphRect) {
            let mut transform = self.transform.borrow_mut();
            let Some(transform) = transform.as_mut() else {
                return;
            };
            if transform.resolved.is_some() {
                return;
            }
            let obj = self.obj();
            transform.resolved = Some(match transform.origin {
                MorphOrigin::Scaled(factor) => bounds.scaled_about_center(factor),
                MorphOrigin::Local(rect) => rect,
                MorphOrigin::Window(rect) => obj
                    .root()
                    .and_then(|root| obj.compute_point(&root, &graphene::Point::zero()))
                    .map_or(bounds.scaled_about_center(0.92), |point| {
                        rect.translated(-point.x(), -point.y())
                    }),
            });
        }

        /// The currently visible shape in local coordinates.
        pub(super) fn current_shape(&self) -> MorphRect {
            let bounds = self.bounds();
            self.resolve_origin(bounds);
            match self.transform.borrow().as_ref() {
                None => bounds,
                Some(transform) => {
                    let origin = transform.resolved.unwrap_or(bounds);
                    let progress = transform.progress.value() as f32;
                    match transform.kind {
                        TransformKind::Expand => origin.lerp(bounds, progress),
                        TransformKind::Collapse => bounds.lerp(origin, progress),
                    }
                }
            }
        }

        pub(super) fn capture(&self) -> Option<Ghost> {
            let obj = self.obj();
            let child = self.child.borrow().clone()?;
            if !obj.is_mapped() || child.width() == 0 || child.height() == 0 {
                return None;
            }
            let bounds = child.compute_bounds(&*obj)?;
            let paintable = gtk::WidgetPaintable::new(Some(&child)).current_image();
            Some(Ghost {
                paintable,
                rect: MorphRect::new(bounds.x(), bounds.y(), bounds.width(), bounds.height()),
            })
        }

        fn snapshot_child(&self, snapshot: &gtk::Snapshot) {
            if let Some(child) = self.child.borrow().as_ref() {
                self.obj().snapshot_child(child, snapshot);
            }
        }

        fn snapshot_ghost(&self, snapshot: &gtk::Snapshot, ghost: &Ghost, x: f32, y: f32) {
            snapshot.save();
            snapshot.translate(&graphene::Point::new(x, y));
            ghost.paintable.snapshot(
                snapshot,
                f64::from(ghost.rect.width),
                f64::from(ghost.rect.height),
            );
            snapshot.restore();
        }

        fn snapshot_crossfade(&self, snapshot: &gtk::Snapshot, bounds: MorphRect) {
            let progress = self.crossfade.borrow().as_ref().map(Spring::value);
            let ghost = self.ghost.borrow();
            match (ghost.as_ref(), progress) {
                (Some(ghost), Some(progress)) => {
                    snapshot.push_opacity(fade_out(progress, 0.0, 0.4));
                    let x = (bounds.width - ghost.rect.width) / 2.0;
                    self.snapshot_ghost(snapshot, ghost, x, ghost.rect.y);
                    snapshot.pop();
                    snapshot.push_opacity(fade_in(progress, 0.2, 1.0));
                    self.snapshot_child(snapshot);
                    snapshot.pop();
                }
                _ => self.snapshot_child(snapshot),
            }
        }

        fn snapshot_transform(
            &self,
            snapshot: &gtk::Snapshot,
            bounds: MorphRect,
            transform: &Transform,
        ) {
            let progress = transform.progress.value();
            let origin = transform.resolved.unwrap_or(bounds);
            let ghost = self.ghost.borrow();
            match transform.kind {
                TransformKind::Expand => {
                    if let Some(ghost) = ghost.as_ref() {
                        snapshot.push_opacity(fade_out(progress, 0.0, 0.4));
                        self.snapshot_ghost(snapshot, ghost, ghost.rect.x, ghost.rect.y);
                        snapshot.pop();
                    }
                    let shape = origin.lerp(bounds, progress as f32);
                    self.snapshot_shaped(snapshot, shape, bounds, origin, fade_in(progress, 0.0, 0.3), |snapshot| {
                        self.snapshot_child(snapshot)
                    });
                }
                TransformKind::Collapse => {
                    let shape = bounds.lerp(origin, progress as f32);
                    let opacity = fade_out(progress, 0.5, 1.0);
                    if transform.capture {
                        snapshot.push_opacity(fade_in(progress, 0.0, 0.6));
                        self.snapshot_child(snapshot);
                        snapshot.pop();
                        if let Some(ghost) = ghost.as_ref() {
                            let full = ghost.rect;
                            self.snapshot_shaped(snapshot, shape, full, origin, opacity, |snapshot| {
                                self.snapshot_ghost(snapshot, ghost, full.x, full.y)
                            });
                        }
                    } else {
                        self.snapshot_shaped(snapshot, shape, bounds, origin, opacity, |snapshot| {
                            self.snapshot_child(snapshot)
                        });
                    }
                }
            }
        }

        /// Draws `content` (laid out at `full`) scaled into `shape` with a rounded clip.
        fn snapshot_shaped(
            &self,
            snapshot: &gtk::Snapshot,
            shape: MorphRect,
            full: MorphRect,
            origin: MorphRect,
            opacity: f64,
            content: impl FnOnce(&gtk::Snapshot),
        ) {
            let end_radius = self.radius.get();
            let start_radius = end_radius.min(origin.height / 2.0);
            let span = (full.width - origin.width).abs().max(1.0);
            let progress = ((shape.width - origin.width).abs() / span).clamp(0.0, 1.0);
            let radius = start_radius + (end_radius - start_radius) * progress;
            let outline = gsk::RoundedRect::from_rect(to_graphene(shape), radius);
            if let Some(shadow) = self.shadow.get() {
                snapshot.append_outset_shadow(
                    &outline,
                    &gdk::RGBA::new(0.0, 0.0, 0.0, shadow.alpha * opacity as f32),
                    0.0,
                    shadow.dy,
                    0.0,
                    shadow.blur,
                );
            }
            snapshot.push_opacity(opacity);
            snapshot.push_rounded_clip(&outline);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(shape.x, shape.y));
            let scale = content_scale(shape, full);
            snapshot.scale(scale, scale);
            snapshot.translate(&graphene::Point::new(-full.x, -full.y));
            content(snapshot);
            snapshot.restore();
            snapshot.pop();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct MorphBin(ObjectSubclass<imp::MorphBin>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for MorphBin {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphBin {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        let imp = self.imp();
        if let Some(previous) = imp.child.take() {
            previous.unparent();
        }
        if let Some(child) = child {
            child.set_parent(self);
            *imp.child.borrow_mut() = Some(child.clone().upcast());
        }
        self.queue_resize();
    }

    pub fn child(&self) -> Option<gtk::Widget> {
        self.imp().child.borrow().clone()
    }

    pub fn set_follow(&self, width: bool, height: bool) {
        self.imp().follow_width.set(width);
        self.imp().follow_height.set(height);
        self.queue_resize();
    }

    pub fn set_start_collapsed(&self, collapsed: bool) {
        self.imp().start_collapsed.set(collapsed);
    }

    pub fn set_radius(&self, radius: f32) {
        self.imp().radius.set(radius);
    }

    pub fn set_shadow(&self, shadow: Option<MorphShadow>) {
        self.imp().shadow.set(shadow);
    }

    pub fn is_animating(&self) -> bool {
        self.imp().is_animating()
    }

    pub fn crossfade(&self, mutate: impl FnOnce()) {
        let imp = imp_with_cleared_callback(self);
        let ghost = if animations_enabled() { imp.capture() } else { None };
        mutate();
        let Some(ghost) = ghost else {
            return;
        };
        *imp.ghost.borrow_mut() = Some(ghost);
        let mut spring = Spring::progress(SpringParams::for_style(MotionStyle::Calm));
        spring.retarget(1.0);
        *imp.crossfade.borrow_mut() = Some(spring);
        imp.ensure_ticking();
        self.queue_resize();
    }

    pub fn expand_from(&self, origin: MorphOrigin, capture: bool, mutate: impl FnOnce()) {
        self.start_transform(TransformKind::Expand, origin, capture, mutate);
    }

    pub fn collapse_into(&self, target: MorphOrigin, capture: bool, mutate: impl FnOnce()) {
        self.start_transform(TransformKind::Collapse, target, capture, mutate);
    }

    fn start_transform(
        &self,
        kind: TransformKind,
        origin: MorphOrigin,
        capture: bool,
        mutate: impl FnOnce(),
    ) {
        let imp = imp_with_cleared_callback(self);
        if !animations_enabled() {
            mutate();
            imp.transform.take();
            imp.ghost.take();
            self.queue_resize();
            return;
        }
        // Interrupting a transform continues from the shape currently on screen.
        let interrupted = imp.transform.borrow().as_ref().map(|transform| {
            (transform.kind, transform.progress.velocity())
        });
        let (origin, velocity) = match interrupted {
            Some((previous, velocity)) if previous != kind && kind == TransformKind::Expand => {
                (MorphOrigin::Local(imp.current_shape()), -velocity)
            }
            // A collapse interrupting an expand restarts from the full bounds toward
            // `origin`; the reversed velocity keeps the momentum continuous.
            Some((previous, velocity)) if previous != kind => (origin, -velocity),
            _ => (origin, 0.0),
        };
        let ghost = if capture { imp.capture() } else { None };
        mutate();
        *imp.ghost.borrow_mut() = ghost;
        let mut progress = Spring::progress(params());
        progress.restart(0.0, 1.0, velocity);
        *imp.transform.borrow_mut() = Some(Transform {
            kind,
            origin,
            resolved: None,
            capture,
            progress,
        });
        imp.ensure_ticking();
        self.queue_resize();
    }

    pub fn on_settled(&self, callback: impl FnOnce() + 'static) {
        let imp = self.imp();
        if imp.is_animating() || imp.tick.borrow().is_some() {
            *imp.settled.borrow_mut() = Some(Box::new(callback));
        } else {
            callback();
        }
    }

    pub fn cancel(&self) {
        let imp = self.imp();
        imp.settled.take();
        if let Some(tick) = imp.tick.take() {
            tick.remove();
        }
        for slot in [&imp.width, &imp.height] {
            if let Some(spring) = slot.borrow_mut().as_mut() {
                let target = spring.target();
                spring.snap(target);
            }
        }
        imp.transform.take();
        imp.ghost.take();
        imp.crossfade.take();
        self.queue_resize();
    }

    pub fn shape_in(&self, ancestor: &impl IsA<gtk::Widget>) -> Option<MorphRect> {
        let shape = self.imp().current_shape();
        let point = self.compute_point(ancestor, &graphene::Point::zero())?;
        Some(shape.translated(point.x(), point.y()))
    }
}

fn imp_with_cleared_callback(bin: &MorphBin) -> &imp::MorphBin {
    let imp = bin.imp();
    imp.settled.take();
    imp
}
```

- [ ] **Step 2: Build**

Run: `cargo build && cargo clippy --all-targets -- -D warnings 2>&1 | grep -v 'never used' | head -40`
Expected: build succeeds. Fix any API mismatch reported by the compiler against gtk4-rs 0.10 (common ones: `compute_point` returns `Option<graphene::Point>`; `PaintableExt::snapshot` takes `&impl IsA<gdk::Snapshot>`; `Widget::root()` returns `Option<gtk::Root>`). Only dead-code warnings for not-yet-used items are acceptable here.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test`
Expected: all existing tests plus Task 1/2 tests pass.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add src/ui/morph_bin.rs src/ui/mod.rs
git commit -m "feat(vigil): add MorphBin spring container widget

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Center island morph

**Files:**
- Modify: `tools/bar/src/ui/surface.rs` — `PrimarySurface` struct (~line 760), `PrimarySurface::new` context stack block (~lines 876-934), `update` context branch (~lines 1120-1172), `context_transition_type` (~line 1780), `apply_motion_style` (~line 1835)

**Interfaces:**
- Consumes: `MorphBin::{new, set_follow, set_child, crossfade}`, `morph::set_motion_style`.
- Produces: `PrimarySurface::island_rect(&self) -> Option<MorphRect>` (monitor coordinates) used in Task 7.

- [ ] **Step 1: Wrap the context stack**

In `PrimarySurface::new`, replace:

```rust
        context_stack.set_transition_duration(180);
        context_stack.set_transition_type(context_transition_type());
```

with:

```rust
        context_stack.set_transition_type(gtk::StackTransitionType::None);
```

Replace the suggestion revealer block:

```rust
        let suggestion_revealer = gtk::Revealer::new();
        suggestion_revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        suggestion_revealer.set_transition_duration(240);
        suggestion_revealer.set_child(Some(&suggestion_actions));
        suggestion_revealer.set_reveal_child(false);
        context_row.append(&suggestion_revealer);
```

with:

```rust
        suggestion_actions.set_visible(false);
        context_row.append(&suggestion_actions);
```

Replace `center_slot.append(&context_stack);` with:

```rust
        let context_morph = MorphBin::new();
        context_morph.set_follow(true, false);
        context_morph.set_child(Some(&context_stack));
        center_slot.append(&context_morph);
```

In the `PrimarySurface` struct replace `suggestion_revealer: gtk::Revealer,` with `suggestion_actions: gtk::Box,` and add `context_morph: MorphBin,` and `center_slot: gtk::Box,`; update the struct literal at the end of `new` accordingly (`suggestion_actions`, `context_morph`, `center_slot`). Add imports at the top of `surface.rs`:

```rust
use super::morph::{self, MorphRect};
use super::morph_bin::MorphBin;
```

Delete `fn context_transition_type()` entirely.

- [ ] **Step 2: Morph on context changes**

In `PrimarySurface::update`, wrap the whole body of `if plan.context { ... }` (everything from `*self.current_context.borrow_mut() = ...` through the final `else { ... }` arm) in a crossfade, and replace the two revealer calls:

```rust
        if plan.context {
            self.context_morph.crossfade(|| {
                *self.current_context.borrow_mut() = spec.context.clone();
                *self.current_suggestion.borrow_mut() = spec.suggestion.clone();
                // ...existing body unchanged, except:
                //   self.suggestion_revealer.set_reveal_child(true);  -> self.suggestion_actions.set_visible(true);
                //   self.suggestion_revealer.set_reveal_child(false); -> self.suggestion_actions.set_visible(false);
            });
        }
```

(The closure only borrows `self` and `spec`; no other changes in the body.)

- [ ] **Step 3: Feed motion style and expose the island rect**

In `apply_motion_style`, add as the first line:

```rust
    morph::set_motion_style(style);
```

Add to `impl PrimarySurface`:

```rust
    /// The center island in monitor coordinates, used as the origin of toast morphs.
    fn island_rect(&self) -> Option<MorphRect> {
        widget_monitor_rect(&self.center_slot)
    }
```

and a free function near `set_context_accessibility`:

```rust
/// A bar widget's bounds in monitor-logical coordinates.
fn widget_monitor_rect(widget: &impl IsA<gtk::Widget>) -> Option<MorphRect> {
    let root = widget.root()?;
    let bounds = widget.compute_bounds(&root)?;
    Some(stage::bar_to_monitor(MorphRect::new(
        bounds.x(),
        bounds.y(),
        bounds.width(),
        bounds.height(),
    )))
}
```

Extend the stage import to `use super::stage::{self, BAR_HEIGHT, NOTIFICATION_GAP, SURFACE_MARGIN};`.

- [ ] **Step 4: Build, test, and smoke test**

Run: `cargo build --release && cargo test`
Expected: pass (dead-code warning for `island_rect` until Task 7 is acceptable).

Visual smoke test (in a live Hyprland session):

```bash
systemctl --user stop vigil.service
./target/release/vigil preview routine &
sleep 3; grim -o "$(hyprctl monitors -j | jq -r '.[0].name')" /tmp/claude-1000/vigil-island-1.png
playerctl play-pause; sleep 0.15; grim /tmp/claude-1000/vigil-island-mid.png
kill %1; systemctl --user start vigil.service
```

Expected: the island width changes smoothly between title and media contexts; the mid-transition screenshot shows a pill at an intermediate width with blended content. Also run `./target/release/vigil preview reduced-motion` and confirm context changes snap with no blending.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add src/ui/surface.rs
git commit -m "feat(vigil): morph the center island between contexts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Control-center open/close morph

**Files:**
- Modify: `tools/bar/src/ui/control_center.rs` — constants (~1096), `ControlCenterMotion` (~1130-1271), `ControlCenterView` struct and `new` (~1307-1400, ~1758-1830), `present`/`dismiss`/`destroy` (~2045-2077), tests (~5003+)
- Modify: `tools/bar/src/ui/surface.rs` — `build_status_button`, `install_context_interactions`, `show_managed_window`, `open_control_center_page`, `close_control_center`, `PrimarySurface` (`system_box` field)
- Modify: `bar/style.css:523-560, 613-619, 650-657` and `.control-center-root` rule
- Modify: `tests/vigil_desktop_contract.sh:326, 452-455, 459`

**Interfaces:**
- Consumes: `MorphBin`, `MorphOrigin`, `MorphShadow`, `stage::{track_input, control_center_stage_width, monitor_to_top_right_stage, panel_top_in_stage, CONTROL_CENTER_RADIUS}`.
- Produces:
  - `ControlCenterView::present_from(&self, origin: Option<MorphRect>)` (origin in monitor coordinates)
  - `ControlCenterView::present(&self)` (= `present_from(None)`)
  - `ControlCenterView::dismiss_now(&self)`
  - `show_managed_window(overlay_id, control_center, coordinator, registry, origin: Option<MorphRect>)`

- [ ] **Step 1: Update the contract test first (failing)**

In `tests/vigil_desktop_contract.sh` replace:

```bash
require_fixed 'window.set_exclusive_zone(0);' "tools/bar/src/ui/control_center.rs"
```
with
```bash
require_fixed 'window.set_exclusive_zone(-1);' "tools/bar/src/ui/control_center.rs"
require_fixed 'panel_morph.set_child(Some(&root));' "tools/bar/src/ui/control_center.rs"
require_fixed 'stage::track_input(&window' "tools/bar/src/ui/control_center.rs"
```

Replace the four keyframe lines (452-455) with:

```bash
reject_fixed '@keyframes control-center-enter {' "bar/style.css"
reject_fixed '@keyframes control-center-exit {' "bar/style.css"
reject_fixed '.control-center-root.control-center-entering {' "bar/style.css"
reject_fixed '.control-center-root.control-center-exiting {' "bar/style.css"
```

Replace `require_fixed 'control_center.present();' "tools/bar/src/ui/surface.rs"` with:

```bash
require_fixed 'control_center.present_from(origin);' "tools/bar/src/ui/surface.rs"
require_fixed 'surface.control_center.dismiss_now();' "tools/bar/src/ui/surface.rs"
```

Run: `bash tests/vigil_desktop_contract.sh` (from the repo root)
Expected: FAIL with `missing required text ... window.set_exclusive_zone(-1);`.

- [ ] **Step 2: Write the failing motion unit test**

In `control_center.rs` tests, add next to the existing `ControlCenterMotionPhase` tests:

```rust
    #[test]
    fn panel_stage_origin_falls_back_in_order() {
        use super::panel_origin;
        use crate::ui::morph::MorphRect;
        use crate::ui::morph_bin::MorphOrigin;

        let button = MorphRect::new(2300.0, 9.0, 30.0, 28.0);
        assert_eq!(
            panel_origin(Some(button), 2560, 512),
            MorphOrigin::Window(MorphRect::new(257.0, 4.0, 30.0, 28.0))
        );
        assert_eq!(panel_origin(None, 2560, 512), MorphOrigin::Scaled(0.92));
    }
```

(`ui::morph` and `ui::morph_bin` are private to `ui`, which still makes them visible to `ui::control_center::tests`; no visibility change is needed.)

Run: `cargo test --lib panel_stage_origin_falls_back_in_order`
Expected: FAIL (`panel_origin` not found).

- [ ] **Step 3: Make the window a stage and wrap the panel**

In `ControlCenterView::new`, replace the window anchoring block:

```rust
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Top, top_margin);
        window.set_margin(Edge::Right, right_margin);
        window.set_exclusive_zone(0);
```

with:

```rust
        // A transparent full-height stage: the panel can grow out of bar items above it
        // without resizing the layer surface; input is limited to the visible shape.
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Top, top_margin);
        window.set_margin(Edge::Right, right_margin);
        window.set_margin(Edge::Bottom, top_margin);
        window.set_exclusive_zone(-1);
```

Replace `window.set_child(Some(&root));` (after the footer is appended) and the `let motion = Rc::new(ControlCenterMotion::new(&window, &root));` line with:

```rust
        let panel_morph = MorphBin::new();
        panel_morph.set_halign(gtk::Align::End);
        panel_morph.set_valign(gtk::Align::Start);
        panel_morph.set_margin_top(stage::panel_top_in_stage());
        panel_morph.set_radius(stage::CONTROL_CENTER_RADIUS);
        panel_morph.set_shadow(Some(MorphShadow {
            dy: 18.0,
            blur: 48.0,
            alpha: 0.44,
        }));
        panel_morph.set_child(Some(&root));
        let stage_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stage_box.append(&panel_morph);
        window.set_child(Some(&stage_box));
        {
            let panel = panel_morph.downgrade();
            stage::track_input(&window, move || {
                panel
                    .upgrade()
                    .and_then(|panel| {
                        let root = panel.root()?;
                        panel.shape_in(&root)
                    })
                    .into_iter()
                    .collect()
            });
        }

        let motion = Rc::new(ControlCenterMotion::new(&window, &root, &panel_morph));
```

Add `stage_box: gtk::Box,` and `panel_morph: MorphBin,` to `ControlCenterView` and its struct literal. Add imports:

```rust
use super::morph::MorphRect;
use super::morph_bin::{MorphBin, MorphOrigin, MorphShadow};
use super::stage;
```

- [ ] **Step 4: Drive `ControlCenterMotion` by the spring**

Delete `CONTROL_CENTER_ENTER_MS` and `CONTROL_CENTER_EXIT_MS`. Replace the `ControlCenterMotion` struct and impl (keep `ControlCenterMotionPhase`/`Event` and their tests unchanged) with:

```rust
struct ControlCenterMotion {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    root: glib::WeakRef<gtk::Box>,
    panel: glib::WeakRef<MorphBin>,
    phase: Cell<ControlCenterMotionPhase>,
    origin: Cell<MorphOrigin>,
}

impl ControlCenterMotion {
    fn new(window: &gtk::ApplicationWindow, root: &gtk::Box, panel: &MorphBin) -> Self {
        Self {
            window: window.downgrade(),
            root: root.downgrade(),
            panel: panel.downgrade(),
            phase: Cell::new(ControlCenterMotionPhase::Hidden),
            origin: Cell::new(MorphOrigin::Scaled(0.92)),
        }
    }

    fn present(self: &Rc<Self>, origin: MorphOrigin) {
        let current = self.phase.get();
        let next = current.transition(ControlCenterMotionEvent::Present);
        if next == current {
            return;
        }
        let (Some(window), Some(root), Some(panel)) =
            (self.window.upgrade(), self.root.upgrade(), self.panel.upgrade())
        else {
            return;
        };
        root.set_sensitive(true);
        self.origin.set(origin);
        if !animations_enabled() {
            panel.cancel();
            self.phase.set(ControlCenterMotionPhase::Visible);
            window.present();
            return;
        }
        self.phase.set(next);
        window.present();
        panel.expand_from(origin, false, || {});
        let motion = Rc::downgrade(self);
        panel.on_settled(move || {
            if let Some(motion) = motion.upgrade() {
                motion.phase.set(
                    motion
                        .phase
                        .get()
                        .transition(ControlCenterMotionEvent::Entered),
                );
            }
        });
    }

    fn dismiss(self: &Rc<Self>) {
        self.dismiss_then(|| {});
    }

    /// Collapses back into the origin, then runs `after` once the panel is hidden.
    fn dismiss_then(self: &Rc<Self>, after: impl FnOnce() + 'static) {
        let current = self.phase.get();
        let next = current.transition(ControlCenterMotionEvent::Dismiss);
        let (Some(window), Some(root), Some(panel)) =
            (self.window.upgrade(), self.root.upgrade(), self.panel.upgrade())
        else {
            after();
            return;
        };
        if next == current {
            after();
            return;
        }
        root.set_sensitive(false);
        if !animations_enabled() {
            self.hide_now();
            after();
            return;
        }
        self.phase.set(next);
        panel.collapse_into(self.origin.get(), false, || {});
        let motion = Rc::downgrade(self);
        panel.on_settled(move || {
            if let Some(motion) = motion.upgrade() {
                motion.phase.set(
                    motion
                        .phase
                        .get()
                        .transition(ControlCenterMotionEvent::Exited),
                );
                if let Some(root) = motion.root.upgrade() {
                    root.set_sensitive(true);
                }
                if let Some(window) = motion.window.upgrade() {
                    window.set_visible(false);
                }
            }
            after();
        });
    }

    /// Hides without animation; used by the session lock and output changes.
    fn hide_now(&self) {
        if let Some(panel) = self.panel.upgrade() {
            panel.cancel();
        }
        self.phase.set(ControlCenterMotionPhase::Hidden);
        if let Some(root) = self.root.upgrade() {
            root.set_sensitive(true);
        }
        if let Some(window) = self.window.upgrade() {
            window.set_visible(false);
        }
    }

    fn is_presented(&self) -> bool {
        self.phase.get().is_presented()
    }

    fn destroy(&self) {
        self.hide_now();
        if let Some(window) = self.window.upgrade() {
            window.close();
        }
    }
}

/// Converts a monitor-space origin into the stage, falling back to a centered scale.
fn panel_origin(origin: Option<MorphRect>, monitor_width: i32, stage_width: i32) -> MorphOrigin {
    origin.map_or(MorphOrigin::Scaled(0.92), |rect| {
        MorphOrigin::Window(stage::monitor_to_top_right_stage(
            rect,
            monitor_width,
            stage_width,
        ))
    })
}
```

- [ ] **Step 5: Update view methods and the footer**

Replace `ControlCenterView::present` and add `present_from` / `dismiss_now`:

```rust
    pub fn present(&self) {
        self.present_from(None);
    }

    /// Opens the panel growing out of `origin` (monitor-logical coordinates).
    pub fn present_from(&self, origin: Option<MorphRect>) {
        let monitor_width = self
            .window
            .monitor()
            .map_or(0, |monitor| monitor.geometry().width());
        let (_, panel_width, _, _) = self.panel_morph.measure(gtk::Orientation::Horizontal, -1);
        let stage_width = stage::control_center_stage_width(panel_width, origin, monitor_width);
        if !self.window.is_visible() {
            self.stage_box.set_size_request(stage_width, -1);
        }
        let stage_width = self.stage_box.width_request().max(stage_width);
        self.motion
            .present(panel_origin(origin, monitor_width, stage_width));
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            self.current_page() == ControlCenterFocus::Bluetooth,
        );
    }

    /// Hides immediately without a morph (session lock, IPC close).
    pub fn dismiss_now(&self) {
        sync_bluetooth_discovery(
            &self.timer_sender,
            &self.current,
            &self.bluetooth_discovery_active,
            false,
        );
        self.motion.hide_now();
    }
```

Note: when the window is already visible the stage width is not changed (it would visibly jump); `width_request()` is what the stage currently uses.

In the footer handler replace `motion_for_footer.dismiss();` and the `glib::timeout_add_local_once(Duration::from_millis(CONTROL_CENTER_EXIT_MS), move || { ... })` wrapper with `motion_for_footer.dismiss_then(move || { ... })` keeping the inner `sender.send(...)` body unchanged.

Search for any remaining use: `grep -n 'CONTROL_CENTER_E' src/ui/control_center.rs` must print nothing.

- [ ] **Step 6: Pass origins from the bar**

In `surface.rs`:

`show_managed_window` gains a parameter and calls `present_from`:

```rust
fn show_managed_window(
    overlay_id: &str,
    control_center: Rc<ControlCenterView>,
    coordinator: Rc<RefCell<PopoverCoordinator>>,
    registry: PopoverRegistry,
    origin: Option<MorphRect>,
) {
    if let Some(previous) = coordinator.borrow_mut().open(overlay_id)
        && previous != overlay_id
        && let Some(active) = registry.borrow().get(&previous).cloned()
    {
        active.hide();
    }
    control_center.present_from(origin);
}
```

In `build_status_button`'s `connect_clicked`, compute the origin and move the panel to the bar's monitor when hidden:

```rust
    button.connect_clicked(move |button| {
        if center_for_click.is_visible() && center_for_click.current_page() == focus {
            center_for_click.dismiss();
            return;
        }
        if !center_for_click.is_visible()
            && let Some(window) = button.root().and_downcast::<gtk::Window>()
            && let Some(monitor) = window.monitor()
        {
            center_for_click.set_monitor(&monitor);
        }
        center_for_click.show_page(focus);
        show_managed_window(
            "control-center",
            center_for_click.clone(),
            coordinator.clone(),
            registry.clone(),
            widget_monitor_rect(button),
        );
    });
```

In `install_context_interactions`, the `OpenControlCenter` arm passes `widget_monitor_rect(&context_row_for_origin)` where `let context_row_for_origin = context_row.clone();` is declared before `primary.connect_pressed`. Update every other `show_managed_window(` call site (`grep -n 'show_managed_window(' src/ui/surface.rs`) to pass `None`.

Add `system_box: gtk::Box,` to `PrimarySurface` (store the existing `system_box`). In `SurfaceRegistry::open_control_center_page` replace `surface.control_center.present();` with:

```rust
        let origin = (target_output == primary_output)
            .then(|| widget_monitor_rect(&surface.system_box))
            .flatten();
        surface.control_center.present_from(origin);
```

In `close_control_center` replace `surface.control_center.dismiss();` with `surface.control_center.dismiss_now();`.

In `SurfaceRegistry::reconcile`, wherever a primary surface's monitor changes or surfaces are rebuilt, the existing `destroy()` path already calls `hide_now()` through `ControlCenterMotion::destroy`; no extra change.

- [ ] **Step 7: CSS**

In `bar/style.css` delete the `@keyframes control-center-enter { ... }` and `@keyframes control-center-exit { ... }` blocks, the `.control-center-root.control-center-entering` / `.control-center-root.control-center-exiting` rules, and the two matching rules inside `@media (prefers-reduced-motion: reduce)`. Remove `transform-origin: 100% 0;` from `.control-center-root`. Keep its `box-shadow` (the `MorphBin` draws the same shadow only while morphing, so rest and motion match).

- [ ] **Step 8: Verify**

Run:
```bash
cargo test && cargo build --release
cd ../.. && bash tests/vigil_desktop_contract.sh && cd tools/bar
```
Expected: all tests pass; contract script exits 0.

Live check:
```bash
systemctl --user restart vigil.service
```
Click the clock, network, and audio status items: the panel must grow out of each item and collapse back into it on Escape/close. Toggle rapidly (double-click a status item): the panel must reverse mid-flight without jumping. Click the bar right beside the open panel: the click must reach the bar (input region pass-through). Run `vigil control-center close` while open: it disappears instantly. Run `vigil control-center toggle` with no button: it morphs from the status cluster. Take a mid-morph screenshot with `vigil control-center open & sleep 0.12; grim /tmp/claude-1000/vigil-cc-mid.png`.

If clicks above/left of the panel are swallowed, input regions are being reset by GTK: confirm with `WAYLAND_DEBUG=1 ./target/release/vigil 2>&1 | grep set_input_region` that requests are sent on each paint; if they are sent but ignored, report back before proceeding.

- [ ] **Step 9: Commit**

```bash
cargo fmt
cd ../..
git add tools/bar/src bar/style.css tests/vigil_desktop_contract.sh
git commit -m "feat(control-center): grow the panel out of its bar origin

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Control-center page morphs

**Files:**
- Modify: `tools/bar/src/ui/control_center.rs` — `NavigationUi` (~721-815), stack creation (~1429-1433), `root.append(&stack)` (~1758), `connect_navigation` (~2470), heading text box (~1405)
- Modify: `tests/vigil_desktop_contract.sh` (stack transition contract)

**Interfaces:**
- Consumes: `MorphBin::{expand_from, collapse_into, crossfade}`, `MorphOrigin::Window`.
- Produces: `NavigationUi::navigate_from(&self, page: ControlCenterFocus, origin: MorphRect)` (origin in window coordinates).

- [ ] **Step 1: Wrap the stack and heading**

Replace:

```rust
        stack.set_transition_duration(180);
```
with
```rust
        stack.set_transition_type(gtk::StackTransitionType::None);
        let page_morph = MorphBin::new();
        page_morph.set_follow(false, true);
        page_morph.set_radius(stage::CARD_RADIUS);
        page_morph.set_child(Some(&stack));
        let heading_morph = MorphBin::new();
        heading_morph.set_hexpand(true);
```

Move `heading_text` into `heading_morph`: replace `header.append(&heading_text);` with:

```rust
        heading_morph.set_child(Some(&heading_text));
        header.append(&heading_morph);
```

Replace `root.append(&stack);` with `root.append(&page_morph);`.

Add to `NavigationUi`:

```rust
    page_morph: MorphBin,
    heading_morph: MorphBin,
    return_origin: Rc<Cell<Option<MorphRect>>>,
```

and initialise them in the `NavigationUi { ... }` literal (`page_morph: page_morph.clone()`, `heading_morph: heading_morph.clone()`, `return_origin: Rc::new(Cell::new(None))`).

- [ ] **Step 2: Rewrite navigation**

Replace `NavigationUi::navigate` with:

```rust
    fn navigate(&self, page: ControlCenterFocus, backwards: bool) {
        self.navigate_with(page, backwards, None);
    }

    /// Expands the page out of the activated tile (window coordinates).
    fn navigate_from(&self, page: ControlCenterFocus, origin: MorphRect) {
        self.navigate_with(page, false, Some(origin));
    }

    fn navigate_with(&self, page: ControlCenterFocus, backwards: bool, origin: Option<MorphRect>) {
        let previous = self.page.get();
        self.page.set(page);
        let show = || self.stack.set_visible_child_name(page.as_str());
        if previous == page || !self.page_morph.is_mapped() {
            self.return_origin.set(None);
            self.page_morph.cancel();
            show();
        } else if let Some(origin) = origin {
            self.return_origin.set(Some(origin));
            self.page_morph
                .expand_from(MorphOrigin::Window(origin), true, show);
        } else if let (true, Some(origin)) = (backwards, self.return_origin.take()) {
            self.page_morph
                .collapse_into(MorphOrigin::Window(origin), true, show);
        } else {
            self.return_origin.set(None);
            self.page_morph.crossfade(show);
        }
        self.back_button
            .set_visible(page != ControlCenterFocus::Overview);
        self.heading_morph.crossfade(|| {
            self.title.set_label(page.title());
            self.subtitle.set_label(page.subtitle());
        });
        self.footer_label
            .set_label(&format!("Open {} in Luma", page.title()));
        self.footer.set_visible(page != ControlCenterFocus::AiUsage);
        self.render_error();
        if let Some(callback) = self.page_changed.borrow().as_ref() {
            callback(page);
        }
    }
```

Replace `connect_navigation`:

```rust
fn connect_navigation(button: &gtk::Button, page: ControlCenterFocus, navigation: &NavigationUi) {
    let navigation = navigation.clone();
    button.connect_clicked(move |button| {
        let origin = button.root().and_then(|root| button.compute_bounds(&root));
        match origin {
            Some(bounds) => navigation.navigate_from(
                page,
                MorphRect::new(bounds.x(), bounds.y(), bounds.width(), bounds.height()),
            ),
            None => navigation.navigate(page, false),
        }
    });
}
```

- [ ] **Step 3: Contract test**

In `tests/vigil_desktop_contract.sh` add after `require_fixed 'stack.set_hhomogeneous(true);' ...`:

```bash
require_fixed 'page_morph.set_child(Some(&stack));' "tools/bar/src/ui/control_center.rs"
reject_fixed 'gtk::StackTransitionType::SlideLeft' "tools/bar/src/ui/control_center.rs"
```

- [ ] **Step 4: Verify**

Run: `cargo test && cargo build --release && (cd ../.. && bash tests/vigil_desktop_contract.sh)`
Expected: pass.

Live check after `systemctl --user restart vigil.service`: open the overview, click the Audio, Power, and Clock summary tiles — each detail page grows out of its tile while the panel height springs to the page; Back collapses it into the same tile. Opening a page directly from a status item shows no page morph (panel morph only). Within the Time page, switching calendar/timers still crossfades (unchanged `view_stack`).

- [ ] **Step 5: Commit**

```bash
cargo fmt
cd ../..
git add tools/bar/src/ui/control_center.rs tests/vigil_desktop_contract.sh
git commit -m "feat(control-center): expand pages out of overview tiles

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Top-center keyed toast morphs

**Files:**
- Modify: `tools/bar/src/ui/surface.rs` — `NotificationStackHandle` (~319), `reconcile_toasts` (~400-494), `clear` (~583), tests (~2676)
- Modify: `bar/style.css` — `.notification-toast` rule (~1883)

**Interfaces:**
- Consumes: `MorphBin`, `MorphOrigin`, `MorphShadow`, `stage::{track_input, toast_origin, toast_top_in_stage, CARD_RADIUS}`, `PrimarySurface::island_rect` (Task 4).
- Produces: `pub(super) struct ToastChanges { removed: Vec<u32>, inserted: Vec<u32>, replaced: Vec<u32> }`, `fn toast_changes(previous: &[Notification], next: &[Notification]) -> ToastChanges`.

- [ ] **Step 1: Write the failing diff test**

In `surface.rs` tests add:

```rust
    #[test]
    fn toast_changes_are_keyed_by_notification_id() {
        use super::{ToastChanges, toast_changes};

        let make = |id: u32, summary: &str| crate::Notification {
            id,
            summary: summary.to_string(),
            ..crate::Notification::default()
        };
        let previous = vec![make(1, "a"), make(2, "b"), make(3, "c")];
        let next = vec![make(4, "d"), make(2, "b2"), make(1, "a")];
        assert_eq!(
            toast_changes(&previous, &next),
            ToastChanges {
                removed: vec![3],
                inserted: vec![4],
                replaced: vec![2],
            }
        );
        assert_eq!(toast_changes(&next, &next), ToastChanges::default());
    }
```

`Notification` (`src/model.rs:174`) does not derive `Default`; add it to the derive list (`#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]`). Every field type already implements `Default` (`NotificationUrgency` defaults to `Normal`).

Run: `cargo test --lib toast_changes_are_keyed_by_notification_id`
Expected: FAIL (`toast_changes` not found).

- [ ] **Step 2: Implement the diff**

Near `notification_stack_width` in `surface.rs`:

```rust
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ToastChanges {
    pub removed: Vec<u32>,
    pub inserted: Vec<u32>,
    pub replaced: Vec<u32>,
}

fn toast_changes(previous: &[Notification], next: &[Notification]) -> ToastChanges {
    let find = |items: &[Notification], id: u32| items.iter().find(|item| item.id == id).cloned();
    ToastChanges {
        removed: previous
            .iter()
            .filter(|item| find(next, item.id).is_none())
            .map(|item| item.id)
            .collect(),
        inserted: next
            .iter()
            .filter(|item| find(previous, item.id).is_none())
            .map(|item| item.id)
            .collect(),
        replaced: next
            .iter()
            .filter(|item| find(previous, item.id).is_some_and(|old| old != **item))
            .map(|item| item.id)
            .collect(),
    }
}
```

Run: `cargo test --lib toast_changes_are_keyed_by_notification_id` → PASS.

- [ ] **Step 3: Replace the stack handle and reconcile**

Replace `NotificationStackHandle` with:

```rust
struct ToastRow {
    morph: MorphBin,
    notification: Notification,
}

type ToastRows = Rc<RefCell<BTreeMap<u32, ToastRow>>>;

struct NotificationStackHandle {
    window: gtk::ApplicationWindow,
    content: gtk::Box,
    rows: ToastRows,
    notifications: Vec<Notification>,
    width: i32,
    output: String,
}
```

In `reconcile_toasts`, keep the existing computation of `notifications` (the `visible_ids` → items mapping). Replace everything from `if notifications.is_empty() {` to the end of the function with:

```rust
        let focused_output = snapshot
            .focused_output
            .as_deref()
            .filter(|output| monitors.contains_key(*output))
            .or_else(|| monitors.keys().next().map(String::as_str));
        let Some(focused_output) = focused_output else {
            return;
        };
        let Some(monitor) = monitors.get(focused_output) else {
            return;
        };
        let monitor_width = monitor.geometry().width();
        let width = notification_stack_width(monitor_width);
        let island = match self.surfaces.get(focused_output) {
            Some(SurfaceHandle::Primary(surface)) => surface.island_rect(),
            _ => None,
        };
        let origin = stage::toast_origin(island, monitor_width, width);

        // Output or width changes rebuild the stack without motion.
        let stale = self.notification_stack.as_ref().is_some_and(|stack| {
            stack.output != focused_output || stack.width != width
        });
        if stale && let Some(stack) = self.notification_stack.take() {
            stack.window.close();
        }
        if notifications.is_empty() && self.notification_stack.is_none() {
            return;
        }
        let animate = !stale;
        let stack = self.notification_stack.get_or_insert_with(|| {
            new_notification_stack(application, monitor, width, focused_output)
        });

        let changes = toast_changes(&stack.notifications, &notifications);
        for id in &changes.removed {
            remove_toast(stack, *id, origin, animate);
        }
        for notification in &notifications {
            if changes.inserted.contains(&notification.id) {
                let morph = MorphBin::new();
                morph.set_follow(false, true);
                morph.set_start_collapsed(animate);
                morph.set_radius(stage::CARD_RADIUS);
                morph.set_shadow(Some(MorphShadow {
                    dy: 8.0,
                    blur: 24.0,
                    alpha: 0.30,
                }));
                morph.set_child(Some(&toast_widget(notification, self.notification_control.as_ref(), width)));
                stack.content.append(&morph);
                if animate {
                    morph.expand_from(MorphOrigin::Window(origin), false, || {});
                }
                stack.rows.borrow_mut().insert(
                    notification.id,
                    ToastRow {
                        morph,
                        notification: notification.clone(),
                    },
                );
            } else if changes.replaced.contains(&notification.id)
                && let Some(row) = stack.rows.borrow_mut().get_mut(&notification.id)
            {
                let widget = toast_widget(notification, self.notification_control.as_ref(), width);
                row.morph.crossfade(|| row.morph.set_child(Some(&widget)));
                row.notification = notification.clone();
            }
        }
        // Order live rows like `notifications`; leaving rows stay where they are.
        let mut previous: Option<gtk::Widget> = None;
        for notification in &notifications {
            if let Some(row) = stack.rows.borrow().get(&notification.id) {
                stack
                    .content
                    .reorder_child_after(&row.morph, previous.as_ref());
                previous = Some(row.morph.clone().upcast());
            }
        }
        stack.notifications = notifications;
        if !stack.notifications.is_empty() && !stack.window.is_visible() {
            stack.window.set_monitor(Some(monitor));
            stack.window.present();
        }
```

Add the helpers below `reconcile_toasts` (as free functions):

```rust
fn new_notification_stack(
    application: &gtk::Application,
    monitor: &gdk::Monitor,
    width: i32,
    output: &str,
) -> NotificationStackHandle {
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title("Vigil Notifications")
        .build();
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(width, -1);
    window.add_css_class("notification-window");
    window.init_layer_shell();
    window.set_namespace(Some("vigil-notifications"));
    window.set_layer(Layer::Overlay);
    // A centered, full-height transparent stage below the island; input follows the cards.
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Top, SURFACE_MARGIN);
    window.set_margin(Edge::Bottom, SURFACE_MARGIN);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.add_css_class("notification-stack");
    content.set_valign(gtk::Align::Start);
    content.set_margin_top(stage::toast_top_in_stage());
    content.set_size_request(width, -1);
    window.set_child(Some(&content));
    let rows: ToastRows = Rc::new(RefCell::new(BTreeMap::new()));
    {
        let content = content.downgrade();
        let window_ref = window.downgrade();
        stage::track_input(&window, move || {
            let (Some(content), Some(window)) = (content.upgrade(), window_ref.upgrade()) else {
                return Vec::new();
            };
            let mut shapes = Vec::new();
            let mut child = content.first_child();
            while let Some(widget) = child {
                if let Some(morph) = widget.downcast_ref::<MorphBin>()
                    && let Some(shape) = morph.shape_in(&window)
                {
                    shapes.push(shape);
                }
                child = widget.next_sibling();
            }
            shapes
        });
    }
    window.present();
    NotificationStackHandle {
        window,
        content,
        rows,
        notifications: Vec::new(),
        width,
        output: output.to_string(),
    }
}

fn toast_widget(
    notification: &Notification,
    control: Option<&NotificationControl>,
    width: i32,
) -> gtk::Box {
    notification_row(
        notification,
        control,
        NotificationPresentation::Toast {
            stack_width: u32::try_from(width).unwrap_or(400),
        },
    )
}

/// Collapses a toast back into the island while its slot springs shut, then removes it.
fn remove_toast(stack: &NotificationStackHandle, id: u32, origin: MorphRect, animate: bool) {
    let Some(row) = stack.rows.borrow_mut().remove(&id) else {
        return;
    };
    let content = stack.content.clone();
    let window = stack.window.downgrade();
    let morph = row.morph;
    let finish = {
        let morph = morph.clone();
        move || {
            content.remove(&morph);
            if content.first_child().is_none()
                && let Some(window) = window.upgrade()
            {
                window.set_visible(false);
            }
        }
    };
    if !animate {
        finish();
        return;
    }
    let child = morph.child();
    morph.collapse_into(MorphOrigin::Window(origin), true, || {
        if let Some(child) = child {
            child.set_visible(false);
        }
    });
    morph.on_settled(finish);
}
```

Update the `clear()` path (line ~583) — it already closes `stack.window`; no change. Delete the old row-rebuild loop (it is fully replaced above). Add imports: `use super::morph_bin::{MorphBin, MorphOrigin, MorphShadow};` (merge with the Task 4 import). `NOTIFICATION_GAP` is no longer used in `surface.rs`; drop it from the stage import so it reads `use super::stage::{self, BAR_HEIGHT, SURFACE_MARGIN};` (clippy runs with `-D warnings`).

- [ ] **Step 4: CSS**

In `bar/style.css`, remove the `animation: cockpit-enter 260ms cubic-bezier(0.16, 1, 0.3, 1) both;` line from `.notification-toast` (the morph replaces it). Keep `cockpit-enter` keyframes if other rules still reference them (`grep -n 'cockpit-enter' bar/style.css`).

- [ ] **Step 5: Verify**

Run: `cargo test && cargo build --release`
Expected: pass.

Live check after `systemctl --user restart vigil.service`:

```bash
notify-send "Morph test" "first"; sleep 0.12; grim /tmp/claude-1000/vigil-toast-mid.png
notify-send "Morph test" "second"; notify-send -u critical "Critical" "stays"
```

Expected: toasts appear top-center below the island, grow out of it, and push older cards down smoothly; dismissing a card collapses it upward into the island while the others slide up; hover still pauses timeouts; the critical card persists; with DND on, normal toasts do not appear. Clicking the desktop beside the toast column must reach the underlying window. Focus a secondary monitor and send a toast: it grows from the top-center fallback pill. Run `./target/release/vigil preview reduced-motion` and send a toast: it appears and disappears without motion.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cd ../..
git add tools/bar/src/ui/surface.rs tools/bar/src/model.rs bar/style.css
git commit -m "feat(notifications): morph toasts out of the center island

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Documentation and full verification

**Files:**
- Modify: `tools/bar/README.md` ("Native notifications" section and a new "Motion" section after "Dotfiles integration")

- [ ] **Step 1: Document**

In `tools/bar/README.md`, in "Native notifications", replace "New notifications appear in a measured three-card stack below the bar on the focused output" with "New notifications grow out of the center island into a measured three-card stack at the top center of the focused output". Add after the "Dotfiles integration" section:

```markdown
## Motion

The center island, quick settings, its detail pages, and notification toasts morph on
springs instead of crossfading or sliding. `experience.motion = "bold"` allows a slight
overshoot; `"calm"` settles without one. Quick settings grows out of the bar item that
opened it and collapses back into it; overview tiles expand into their detail pages.
Interrupting a morph reverses it from the shape on screen. Disabling GTK animations
(`gtk-enable-animations=false`) makes every change instant, and
`vigil control-center close` always hides immediately so nothing lingers above the lock
screen. Quick settings and toasts use transparent overlay surfaces whose input region
follows the visible shapes, so clicks beside them reach the windows underneath.
```

- [ ] **Step 2: Full verification**

Run from the repository root:

```bash
(cd tools/bar && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release)
bash tests/vigil_desktop_contract.sh
bash tests/session_lock_test.sh
bash -n setup.sh scripts/*.sh
systemctl --user restart vigil.service && sleep 2 && journalctl --user -u vigil.service --since "-1min" --no-pager | grep -Ei 'critical|warning|gtk' || true
```

Expected: everything passes; the journal shows no new GTK criticals (in particular no `gtk_widget_snapshot_child` or allocation warnings).

- [ ] **Step 3: Screenshot set and user check**

Capture mid-morph screenshots for: island context change, panel open from the clock, tile→page, toast arrival — under both `experience.motion = "bold"` and `"calm"` (edit `~/.config/vigil/config.toml`, which reloads live). Save to the scratchpad and show them to the user. Ask the user to confirm the feel on the three-monitor Hyprland layout and on Niri before considering the work done (per AGENTS.md: ask for feedback on behavior changes).

- [ ] **Step 4: Commit**

```bash
git add tools/bar/README.md
git commit -m "docs(vigil): describe morphing transitions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
