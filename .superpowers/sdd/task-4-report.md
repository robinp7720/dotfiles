# Task 4 Report: Structured And Prioritizable Calendar State

## Scope

- Reworked `scripts/next_event.sh` so the cache stores structured calendar JSON and `--json` exposes the same record used by legacy text and Waybar output.
- Added deterministic backend selection with `CALENDAR_BACKEND=auto|khal|gcalcli` and deterministic time control through `NEXT_EVENT_NOW_EPOCH`.
- Added isolated shell coverage in `scripts/tests/next_event_test.sh` for fake `gcalcli`, JSON output, cache contents, and Waybar compatibility.
- Added `tools/bar/src/sources/calendar.rs` and `tools/bar/src/sources/mod.rs` with a typed parser and polling source boundary that publishes `SourceHealth` separately from calendar state.
- Added Rust integration coverage in `tools/bar/tests/calendar.rs` for parser validation, healthy event publication, and disconnected-health behavior that leaves fresh prior calendar state intact.

## TDD Evidence

### RED

Shell red command:

```bash
bash scripts/tests/next_event_test.sh
```

Observed failure:

- `json.decoder.JSONDecodeError: Expecting value...` because `scripts/next_event.sh` did not yet support `--json`.

Rust red command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml calendar
```

Observed failure:

- `unresolved imports cockpit_bar::parse_calendar_json, cockpit_bar::spawn_calendar_source`

These were the expected missing-feature failures.

### GREEN

Focused green commands:

```bash
bash scripts/tests/next_event_test.sh
cargo test --manifest-path tools/bar/Cargo.toml calendar
```

Results:

- Shell test: `ok - next_event.sh structured calendar contract passed`
- Rust calendar filter: 10 calendar-matching tests passed across existing unit tests plus the new integration tests

## Full Verification

Commands:

```bash
cargo fmt --manifest-path tools/bar/Cargo.toml
bash scripts/tests/next_event_test.sh
cargo test --manifest-path tools/bar/Cargo.toml calendar
cargo test --manifest-path tools/bar/Cargo.toml
bash -n scripts/next_event.sh scripts/tests/next_event_test.sh
git diff --check
```

Results:

- `cargo test --manifest-path tools/bar/Cargo.toml`: 33 passed, 0 failed
- Shell syntax check: clean
- `git diff --check`: clean

## Self-Review

- Legacy consumers still get the prior text/Waybar behavior, while `--json` now distinguishes healthy empty state from backend failure.
- The script keeps backend preference compatible with the old path by trying `khal` first and falling through to `gcalcli` only when `khal` is present but empty.
- Rust parsing rejects invalid timestamps before they can become `CalendarEvent` state.
- The source thread only sends `StateUpdate::Health` on unhealthy backend output, so a fresh prior event remains visible until the normal freshness expiry path removes it.
- The source boundary intentionally is not wired into startup yet; that stays for later source/supervisor tasks.

## Task 4 Review Fixes

### Findings Addressed

- Replaced khal's default human-output parser with an explicit khal 0.14-compatible format using `start-long-full`, `end-long-full`, `uid`, `title`, and `location`, with day headings disabled and `LC_ALL=C` for stable locale defaults.
- Changed `CALENDAR_BACKEND=auto` to continue to gcalcli after either khal command failure or khal parse failure.
- Normalized omitted backend ends to the start epoch and made `CalendarRecord.end_epoch` a required `i64` validated as positive and not before the start.
- Removed manual JSON escaping. Event, empty, error, cache, and Waybar payloads are now serialized with Python's standard `json` module; gcalcli TSV is decoded with Python's standard `csv` module.
- Added isolated coverage for all-day khal records, fallback after malformed khal output, missing ends, invalid Rust ends, and quotes/tab/carriage-return/newline/backslash round trips.

The worktree did not have `khal` installed. `pacman -Si khal` identified the current Arch package as 0.14.0-1; the matching upstream 0.14.0 documentation and source confirmed the selected `--format` fields, `*-full` all-day behavior, `--day-format`, and `--once` support.

### Review Fix RED Evidence

Shell regressions were added before production changes and run independently:

```bash
bash scripts/tests/next_event_test.sh khal-fallback
bash scripts/tests/next_event_test.sh khal-all-day
bash scripts/tests/next_event_test.sh missing-end
bash scripts/tests/next_event_test.sh control-characters
```

Observed failures:

- khal fallback: `expected title='Fallback review', got None`
- khal all-day/format contract: `expected id='all-day-uid', got None`
- missing backend end: `expected end_epoch='1800000600', got None`
- unsafe control characters: `json.decoder.JSONDecodeError: Invalid control character`

The locale assertion was also introduced before `LC_ALL=C` and failed with `expected id='all-day-uid', got None` because the fake khal rejected the invocation environment.

Rust RED command:

```bash
cargo test --manifest-path tools/bar/Cargo.toml --test calendar
```

Observed failure:

- `error[E0308]: mismatched types` because the test expected integer `end_epoch`, while `CalendarRecord` still exposed `Option<i64>`.

### Review Fix GREEN Evidence

Focused GREEN commands:

```bash
bash scripts/tests/next_event_test.sh
cargo test --manifest-path tools/bar/Cargo.toml --test calendar
```

Results:

- Shell suite: `ok - next_event.sh structured calendar contract passed`
- Rust calendar integration suite: 7 passed, 0 failed

### Review Fix Full Verification

Commands run after formatting and self-review:

```bash
cargo fmt --manifest-path tools/bar/Cargo.toml
bash scripts/tests/next_event_test.sh
cargo test --manifest-path tools/bar/Cargo.toml --test calendar
cargo test --manifest-path tools/bar/Cargo.toml
bash -n scripts/next_event.sh scripts/tests/next_event_test.sh
cargo fmt --manifest-path tools/bar/Cargo.toml --check
git diff --check
```

Results:

- Full crate: 36 passed, 0 failed (29 unit tests and 7 calendar integration tests)
- Shell syntax: clean
- Rust formatting check: clean
- Diff whitespace check: clean

### Review Fix Changed Files

- `scripts/next_event.sh`
- `scripts/tests/next_event_test.sh`
- `tools/bar/src/sources/calendar.rs`
- `tools/bar/tests/calendar.rs`

### Review Fix Self-Review

- Existing text, `--blank-when-empty`, cache reuse, and `--waybar` rendering still flow through the same cached record and retain their prior behavior for gcalcli consumers.
- Malformed khal output no longer masks a working gcalcli backend in auto mode.
- Every newly generated healthy event record has a positive numeric end; Rust rejects missing, zero, negative, or reversed ends.
- Khal is not installed in this environment, so khal behavior is verified with isolated fake-backend contract tests against the documented 0.14.0 CLI rather than a live calendar database.

## Task 4 Re-review 2 Fix

### Re-review 2 RED Evidence

The new regression was added first in `scripts/tests/next_event_test.sh` for fake `gcalcli` output with an empty `start_time` and a present `start_date`, `summary`, and `location`.

Command:

```bash
bash scripts/tests/next_event_test.sh date-only-gcalcli
```

Observed failure:

- `not ok - expected legacy text output, got: No upcoming events`

Root cause confirmed from the structured `gcalcli` parser: it raised `ValueError("gcalcli start time is missing")`, which converted the row into unhealthy JSON and drove legacy text, `--blank-when-empty`, and `--waybar` through the empty/no-event path. The pre-structured script only skipped ETA generation when `start_time` was empty and still rendered `summary` plus optional `location`.

### Re-review 2 GREEN Evidence

Minimal fix:

- For `gcalcli` rows with a missing `start_time` but present `start_date`, derive `start_epoch` from the date-only value.
- Continue normalizing a missing end to the start epoch.
- Render text as `title` plus optional `at location`, without ETA, for that legacy row shape.

Focused verification:

```bash
bash scripts/tests/next_event_test.sh date-only-gcalcli
```

Result:

- `ok - next_event.sh structured calendar contract passed`

### Re-review 2 Full Verification

Commands:

```bash
bash scripts/tests/next_event_test.sh
bash -n scripts/next_event.sh scripts/tests/next_event_test.sh
cargo test --manifest-path tools/bar/Cargo.toml --test calendar
git diff --check
```

Results:

- Shell suite: `ok - next_event.sh structured calendar contract passed`
- Syntax check: clean
- Rust calendar integration test: 7 passed, 0 failed
- Diff whitespace check: clean
