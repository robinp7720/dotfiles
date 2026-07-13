## What you implemented

- Added `tools/bar/src/ui/system.rs` with:
  - `SystemCluster` presentation data for keyboard, resources, network, Bluetooth/audio, power, and clock modules.
  - Button labels, icon selection, CSS classes, tooltips, popover lines, inline controls, and Luma footer actions.
  - Presentation coverage for stale source health, disconnected dependencies, media state, and popover-local action failures.
- Added `tools/bar/src/ui/popovers.rs` with `PopoverCoordinator` to:
  - Track the active popover ID.
  - Close the previously active popover when another opens.
  - Clear the active popover on Escape.
  - Keep action failures attached to the originating system popover until the next action or close.
- Updated `tools/bar/src/ui/surface.rs` to:
  - Render the system module cluster on the primary surface only.
  - Keep reduced surfaces limited to workspaces, title, warnings, and clock.
  - Use GTK popovers for system modules and title popovers with shared mutual exclusion.
  - Add conventional scroll controllers for workspaces, keyboard, media, and power profile.
  - Rebuild active system popovers when action completions fail so errors stay local to the popover.
- Updated `tools/bar/src/ui/mod.rs` to export the new UI modules and feed action completions into the surface registry.

## TDD Evidence

### RED

Command from the brief:

```bash
cargo test --manifest-path tools/bar/Cargo.toml ui::system ui::popovers
```

Output:

```text
error: unexpected argument 'ui::popovers' found

Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]
```

Equivalent failing per-module check used to observe the missing implementation:

```bash
cargo test --manifest-path tools/bar/Cargo.toml ui::system
```

Output excerpt before implementation:

```text
error[E0432]: unresolved imports `super::SystemModuleId`, `super::build_popover_spec`, `super::build_system_cluster`
error[E0432]: unresolved import `super::PopoverCoordinator`
error: could not compile `cockpit-bar` (lib test) due to previous errors
```

### GREEN

Commands after implementation:

```bash
cargo test --manifest-path tools/bar/Cargo.toml ui::system
cargo test --manifest-path tools/bar/Cargo.toml ui::popovers
```

Output summary:

```text
ui::system: 7 passed, 0 failed
ui::popovers: 2 passed, 0 failed
```

## Final verification commands and results

```bash
cargo test --manifest-path tools/bar/Cargo.toml ui
cargo build --manifest-path tools/bar/Cargo.toml
```

Results:

- `cargo test --manifest-path tools/bar/Cargo.toml ui`: PASS (`21 passed, 0 failed`)
- `cargo build --manifest-path tools/bar/Cargo.toml`: PASS

## Files changed

- `tools/bar/src/ui/mod.rs`
- `tools/bar/src/ui/surface.rs`
- `tools/bar/src/ui/system.rs`
- `tools/bar/src/ui/popovers.rs`

## Self-review findings

- Kept the implementation inside the requested UI files only.
- Preserved reduced-surface scope: no system cluster was added there.
- Routed action-failure rendering through the UI layer only; no desktop notifications were added.
- Shared one coordinator per surface so title and system popovers do not stack on top of each other.

## Any issues or concerns

- The exact RED command in the brief is not accepted by the current Cargo CLI because it only accepts one test filter argument. I preserved that output in this report and used equivalent per-module commands to complete the failing-test step.

## Review Fix Follow-up

### Findings fixed

- Critical: kept active `system:*` popovers stable across ordinary `plan.system` refreshes by preserving the active system popover through rebuild teardown instead of treating every refresh like an explicit close.
- Important: added lifecycle/policy coverage in `ui::popovers` for the real rebuild behavior used by `PrimarySurface`, including both the preserve-on-refresh case and the removed-module cleanup case.
- Minor: added an explicit reduced-surface assertion that reduced specs keep `system: None`.

### Files changed

- `tools/bar/src/ui/popovers.rs`
- `tools/bar/src/ui/surface.rs`
- `.superpowers/sdd/task-11-report.md`

### RED/GREEN evidence

- RED:
  - `cargo test --manifest-path tools/bar/Cargo.toml ui::popovers::tests::system_rebuild_preserves_active_popover_and_error_for_existing_module`
  - Result: FAIL before the fix with `no method named 'prepare_system_popover_rebuild' found for struct 'PopoverCoordinator'`
- GREEN:
  - `cargo test --manifest-path tools/bar/Cargo.toml ui::popovers::tests::system_rebuild_preserves_active_popover_and_error_for_existing_module`
  - Result: PASS (`1 passed, 0 failed`)

### Final verification commands/results

- `cargo test --manifest-path tools/bar/Cargo.toml ui`
  - PASS (`24 passed, 0 failed`)
- `cargo build --manifest-path tools/bar/Cargo.toml`
  - PASS
- `cargo fmt --manifest-path tools/bar/Cargo.toml --check`
  - PASS
- `git diff --check`
  - PASS

### Self-review/concerns

- The fix stays inside the Task 11 UI files and keeps the existing rebuild model; the coordinator now distinguishes refresh teardown from explicit user close for active system popovers.
- The new policy test is headless and directly exercised by the `PrimarySurface` render path via `prepare_system_popover_rebuild`, which closes the regression gap without introducing GTK-only test fragility.
- No additional concerns from this scoped follow-up.
