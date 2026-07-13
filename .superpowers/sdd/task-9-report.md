# Task 9 Report

## Changed files

- `tools/bar/src/actions.rs`
  - Added `ActionBackend`, `ActionRouter`, `ActionResult`, `ActionCompletion`, `ProcessSpec`, and `SystemActionBackend`.
  - Routed compositor, media, power-profile, timer, keyboard-layout, and Luma-query intents.
  - Added backend-spy tests covering compositor routing, media scroll routing, fixed power-profile cycling, Luma query argv, typed timer requests, and failure mapping.
- `tools/bar/src/lib.rs`
  - Exported the new actions module and its public types for later UI tasks.

## TDD red/green evidence

### RED

Command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml actions
```

Result:

- Failed as required before implementation.
- Compiler error: unresolved imports from `actions` in `src/lib.rs`, including `ActionBackend`, `ActionCompletion`, `ActionResult`, `ActionRouter`, `ProcessSpec`, and `spawn_action_worker`.
- This confirmed the focused test surface was pointed at the missing Task 9 API.

### GREEN

Command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml actions
```

Result:

- Passed after implementation.
- `7 passed; 0 failed`.
- Covered:
  - workspace clicks -> compositor backend
  - media scroll -> `playerctl previous/next`
  - fixed power-profile cycling
  - title secondary-click -> `Luma --query windows`
  - context secondary-click -> `Luma --query <card-query>`
  - typed timer control requests
  - backend failure -> `ActionResult::Failed { summary, detail }`

## Verification commands and results

### Focused action tests

```bash
cargo test --manifest-path tools/bar/Cargo.toml actions
```

- Pass
- `7 passed; 0 failed`

### Full bar test suite

```bash
cargo test --manifest-path tools/bar/Cargo.toml
```

- Pass
- `57` library tests passed
- `3` main tests passed
- `7` calendar integration tests passed
- `8` compositor integration tests passed

### Formatting

```bash
cargo fmt --manifest-path tools/bar/Cargo.toml --check
```

- Initial run failed on formatting only.
- Ran `cargo fmt --manifest-path tools/bar/Cargo.toml`.
- Re-ran `--check`: pass.

### Diff hygiene

```bash
git diff --check
```

- Pass

## Self-review

- Kept the patch inside the requested Task 9 source files plus this report.
- Used direct argv for routed commands; no `sh -c`, shell-expanded command strings, or bundled shell command payloads.
- Made `launch_process` use `spawn()` so `Luma` launches do not block the worker thread until exit.
- Kept media and power-profile commands as synchronous service commands so failures can surface as `ActionResult::Failed`.
- Kept power-profile cycling deterministic with a fixed `power-saver -> balanced -> performance` order and updated router-local state only after a successful backend call.
- Returned a typed completion hook (`spawn_action_worker`) without coupling it to GTK startup or GLib.

## Concerns

- The async completion hook is ready, but Task 10/11 still needs to connect it to the UI channel and popover error presentation.
- When UI wiring lands, power-profile actions need to seed `ActionRouter::with_power_profile_state(snapshot.system.power.profile.clone())` from the latest snapshot so scroll behavior stays aligned with live state.

## Review fix follow-up

### Findings addressed

- Replaced the single-intent `spawn_action_worker` API with a persistent worker thread that owns one `ActionRouter` and processes typed `ActionRequest` values over a channel, so router-local power-profile state survives across requests on the public threaded path.
- Added an explicit caller-supplied `origin` token to both `ActionRequest` and `ActionCompletion`, so later UI wiring can route completions and failures back to the originating popover deterministically.

### RED

Command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml actions
```

Result:

- Failed before implementation.
- Errors matched the old worker API shape:
  - missing `origin` field in `ActionCompletion`
  - `spawn_action_worker` still required a single `ActionIntent`
  - the new tests expected a persistent request sender plus join handle

### GREEN

Command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml actions
```

Result:

- Passed after implementation.
- `9 passed; 0 failed`.
- New coverage added:
  - threaded worker preserves seeded power-profile state across two cycle requests
  - threaded completion carries the caller-supplied origin token unchanged
