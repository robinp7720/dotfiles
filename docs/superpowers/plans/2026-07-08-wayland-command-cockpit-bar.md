# Wayland Command Cockpit Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust GTK/layer-shell bar for Hyprland and Niri with output-local WM context, prioritized dynamic activity, interactive system controls, and reduced secondary-output surfaces.

**Architecture:** One `cockpit-bar` process owns compositor adapters, independent state sources, a typed state store, a pure context arbiter, GTK surfaces, and a user-only Unix control socket. Background workers publish normalized updates through channels; GTK renders immutable snapshots and routes actions through testable backends.

**Tech Stack:** Rust 2024, GTK4 0.10.1, gtk4-layer-shell 0.7.1, serde/serde_json, TOML, blocking zbus workers, standard Unix sockets/channels, Bash, Zsh, Matugen, systemd user services

## Global Constraints

- Support Wayland sessions running Hyprland or Niri; keep Polybar unchanged for bspwm/X11.
- Run one process per session, with exactly one full primary bar and reduced bars on all other connected outputs.
- Reduced bars contain output-local workspaces, output-local active window title, clock, and critical warnings only.
- Priority order is critical, imminent, work, then ambient; calendar defaults to 15 minutes, timer defaults to 5 minutes, battery defaults to low at 15% and critical at 7% while discharging.
- Use native GTK popovers for immediate controls and Luma for deeper or multi-step workflows.
- A failed source must degrade only its own module and must not block GTK rendering.
- Edit `matugen/templates/bar-colors.css`, never generated `bar/colors.css` directly.
- Keep the current Waybar launch path available until Hyprland and Niri live acceptance passes.
- Use Rust for bar logic and shell only for calendar and shell-integration glue.

---

## File Map

### Rust crate

- `tools/bar/Cargo.toml`: crate metadata and dependencies.
- `tools/bar/src/main.rs`: CLI dispatch and process entrypoint.
- `tools/bar/src/lib.rs`: module exports and application startup.
- `tools/bar/src/model.rs`: normalized state, update, health, and action types.
- `tools/bar/src/config.rs`: TOML loading, defaults, validation, and reloadable settings.
- `tools/bar/src/state.rs`: deduplicating state store and freshness expiry.
- `tools/bar/src/context.rs`: pure priority and dismissal policy.
- `tools/bar/src/ipc.rs`: user-only Unix socket protocol and client/server code.
- `tools/bar/src/timers.rs`: persisted countdown state and operations.
- `tools/bar/src/activity.rs`: build/test lifecycle state.
- `tools/bar/src/compositor/{mod.rs,hyprland.rs,niri.rs}`: shared adapter contract and compositor implementations.
- `tools/bar/src/sources/{mod.rs,power.rs,resources.rs,network.rs,bluetooth.rs,audio.rs,media.rs,calendar.rs}`: independently supervised system sources.
- `tools/bar/src/actions.rs`: action routing through injectable backends.
- `tools/bar/src/ui/{mod.rs,surface.rs,wm.rs,context_card.rs,system.rs,popovers.rs,theme.rs}`: GTK rendering split by visible responsibility.
- `tools/bar/tests/fixtures/`: recorded compositor, calendar, and command output fixtures.

### Desktop integration

- `bar/config.toml`: tracked user configuration; prefer `DP-5` as primary with focused-output fallback.
- `bar/style.css`: stable GTK geometry and semantic state classes.
- `bar/shell-integration.zsh`: opt-in command activity hooks.
- `bar/tests/shell-integration.zsh`: isolated Zsh contract tests.
- `matugen/templates/bar-colors.css`: Matugen color role output.
- `matugen/config.toml`: generate `~/.config/cockpit-bar/colors.css`.
- `scripts/next_event.sh`: add structured calendar output without breaking current modes.
- `scripts/tests/next_event_test.sh`: fake-backend calendar contract test.
- `systemd/user/cockpit-bar.service`: supervised session process.
- `setup.sh`, `zshrc`, `hypr/hyprland-config/startup.conf`, `hypr/hyprland-config/base.conf`, `niri/config.kdl`: installation and startup integration.

---

### Task 1: Establish The Crate, Domain Model, And Configuration

**Files:**
- Create: `tools/bar/Cargo.toml`
- Create: `tools/bar/src/lib.rs`
- Create: `tools/bar/src/main.rs`
- Create: `tools/bar/src/model.rs`
- Create: `tools/bar/src/config.rs`
- Create: `bar/config.toml`

**Interfaces:**
- Produces: `AppConfig::load(path: &Path) -> Result<AppConfig>`, `OutputRole`, `BarSnapshot`, `StateUpdate`, `SourceHealth`, `ActionIntent`.
- Consumes: no earlier task interfaces.

- [ ] **Step 1: Write failing configuration and output-role tests**

Add tests in `config.rs` that parse the tracked shape and reject invalid threshold ordering:

```rust
#[test]
fn defaults_match_the_approved_urgency_policy() {
    let config = AppConfig::default();
    assert_eq!(config.primary_output.as_deref(), Some("DP-5"));
    assert_eq!(config.thresholds.calendar_soon_minutes, 15);
    assert_eq!(config.thresholds.timer_soon_minutes, 5);
    assert_eq!(config.thresholds.battery_low_percent, 15);
    assert_eq!(config.thresholds.battery_critical_percent, 7);
}

#[test]
fn critical_battery_must_be_below_low_battery() {
    let text = "[thresholds]\nbattery_low_percent=7\nbattery_critical_percent=15\n";
    let error = AppConfig::from_toml(text).unwrap_err().to_string();
    assert!(error.contains("battery_critical_percent must be lower"));
}
```

- [ ] **Step 2: Run the tests and verify the crate is absent**

Run: `cargo test --manifest-path tools/bar/Cargo.toml config`

Expected: FAIL because `tools/bar/Cargo.toml` does not exist.

- [ ] **Step 3: Add the crate and minimal typed model**

Use the repo's existing Rust versions and add `anyhow`, `clap` with derive,
`dirs`, `gtk4 = 0.10.1` with `v4_20`, `gtk4-layer-shell = 0.7.1`, `serde`,
`serde_json`, `toml`, `thiserror`, `tracing`, `tracing-subscriber`, and `zbus`
with its blocking API. Define these core types exactly:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarSnapshot {
    pub outputs: BTreeMap<String, OutputState>,
    pub focused_output: Option<String>,
    pub system: SystemState,
    pub activities: ActivityState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRole { Primary, Reduced }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceHealth { Healthy, Stale { since_epoch: i64 }, Disconnected { message: String } }

#[derive(Clone, Debug, PartialEq)]
pub enum StateUpdate {
    Outputs(Vec<OutputState>),
    FocusedOutput(Option<String>),
    System(SystemUpdate),
    Activity(ActivityUpdate),
    Health { source: SourceId, health: SourceHealth },
}
```

`AppConfig::from_toml` must merge omitted fields with defaults, validate
percentages in `0..=100`, require critical battery below low battery, and reject
zero retry/freshness durations. `main.rs` should parse the CLI and print errors
with a non-zero exit; application startup remains a stub returning `Ok(())`.

- [ ] **Step 4: Add the tracked default configuration**

Create `bar/config.toml` with `primary_output = "DP-5"`, the four approved
thresholds, full/reduced module lists, stale deadlines, and the initial command
activity allowlist: Cargo build/test/run, npm/pnpm test, pytest, and `make`.

- [ ] **Step 5: Run formatting and tests**

Run: `cargo fmt --manifest-path tools/bar/Cargo.toml --check && cargo test --manifest-path tools/bar/Cargo.toml config`

Expected: PASS with both configuration tests green.

- [ ] **Step 6: Commit**

```bash
git add tools/bar bar/config.toml
git commit -m "feat(bar): establish typed core and configuration"
```

### Task 2: Implement The State Store And Context Arbiter

**Files:**
- Create: `tools/bar/src/state.rs`
- Create: `tools/bar/src/context.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `tools/bar/src/model.rs`

**Interfaces:**
- Consumes: `BarSnapshot`, `StateUpdate`, `ThresholdConfig` from Task 1.
- Produces: `StateStore::apply`, `StateStore::expire`, `select_context`, `ContextCard`, `Dismissals`.

- [ ] **Step 1: Write failing policy tests**

Cover a low-battery override, a 10-minute calendar event beating work, a
completed timer beating both, dismissal until severity changes, and restoration
of the previous card. Use fixed Unix timestamps, not wall-clock sleeps:

```rust
#[test]
fn imminent_calendar_beats_running_build() {
    let now = 1_800_000_000;
    let snapshot = fixture_snapshot()
        .with_build("cargo test", ActivityStatus::Running)
        .with_event("review", now + 10 * 60);
    assert!(matches!(
        select_context(&snapshot, now, &ThresholdConfig::default(), &Dismissals::default()),
        Some(ContextCard::Calendar { ref id, .. }) if id == "review"
    ));
}
```

- [ ] **Step 2: Run the tests to verify missing policy code**

Run: `cargo test --manifest-path tools/bar/Cargo.toml context::tests state::tests`

Expected: FAIL with unresolved `select_context` and `StateStore`.

- [ ] **Step 3: Implement deterministic scoring and state expiry**

Define explicit ascending numeric ranks so `max_by` selects `Critical` over
`Imminent`, `Work`, and `Ambient`. Wrap the action deadline in `Reverse`, using
`i64::MAX` when no deadline exists, so the soonest actionable item wins inside
a tier; use `changed_at` only as the final tie-breaker. Model critical battery
only while discharging. Model timer completion as critical, the last five
minutes as imminent, and calendar events inside 15 minutes as imminent. Store
dismissals by stable item key and severity generation:

```rust
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextTier {
    Ambient = 0,
    Work = 1,
    Imminent = 2,
    Critical = 3,
}

fn priority_key(candidate: &Candidate) -> (u8, Reverse<i64>, i64) {
    (
        candidate.tier as u8,
        Reverse(candidate.action_deadline.unwrap_or(i64::MAX)),
        candidate.changed_at,
    )
}

pub fn select_context(
    snapshot: &BarSnapshot,
    now_epoch: i64,
    thresholds: &ThresholdConfig,
    dismissals: &Dismissals,
) -> Option<ContextCard> {
    candidates(snapshot, now_epoch, thresholds)
        .into_iter()
        .filter(|candidate| !dismissals.suppresses(candidate, now_epoch))
        .max_by_key(priority_key)
        .map(|candidate| candidate.card)
}
```

`StateStore::apply` returns `false` for equivalent updates and `true` for a
meaningful mutation. `expire(now)` changes overdue source health to `Stale` and
clears values whose stale policy says hiding is safer than displaying them.

- [ ] **Step 4: Run focused and full tests**

Run: `cargo test --manifest-path tools/bar/Cargo.toml context::tests state::tests && cargo test --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tools/bar/src
git commit -m "feat(bar): prioritize dynamic context state"
```

### Task 3: Add The Control Socket And Persistent Timers

**Files:**
- Create: `tools/bar/src/ipc.rs`
- Create: `tools/bar/src/timers.rs`
- Modify: `tools/bar/src/main.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `tools/bar/src/model.rs`

**Interfaces:**
- Consumes: `ActivityUpdate`, `StateUpdate` from Tasks 1-2.
- Produces: `ControlRequest`, `ControlResponse`, `ControlClient::send`, `TimerStore::{load,apply,snapshot}`.

- [ ] **Step 1: Write failing protocol and restart tests**

Test serde round-trips, socket path selection, `0600` socket permissions, and a
timer surviving reload from a temporary state directory. Include pause/resume
math and completion across restart.

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ipc::tests timers::tests`

Expected: FAIL because the modules do not exist.

- [ ] **Step 3: Implement the newline-delimited JSON protocol**

Use `${XDG_RUNTIME_DIR}/cockpit-bar.sock`, reject a missing runtime directory,
remove only a socket owned by the current user, and set mode `0600`. Define:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    TimerStart { label: String, duration_seconds: u64 },
    TimerPause { id: String },
    TimerResume { id: String },
    TimerCancel { id: String },
    TimerList,
    ActivityStart { id: String, label: String, cwd: PathBuf, started_at: i64 },
    ActivityFinish { id: String, exit_code: i32, finished_at: i64 },
}
```

Persist `TimerRecord` atomically to
`${XDG_STATE_HOME:-~/.local/state}/cockpit-bar/timers.json` using a temporary
file plus rename. The CLI forms are `cockpit-bar timer start 25m --label Focus`,
`timer pause ID`, `timer resume ID`, `timer cancel ID`, and `timer list`.

- [ ] **Step 4: Run protocol tests and a local CLI smoke test**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ipc::tests timers::tests`

Expected: PASS. Then start the test server fixture and verify `timer list`
returns JSON containing `"timers"` and exits zero.

- [ ] **Step 5: Commit**

```bash
git add tools/bar/src
git commit -m "feat(bar): add persistent timer control protocol"
```

### Task 4: Make Calendar State Structured And Prioritizable

**Files:**
- Modify: `scripts/next_event.sh`
- Create: `scripts/tests/next_event_test.sh`
- Create: `tools/bar/src/sources/calendar.rs`
- Modify: `tools/bar/src/sources/mod.rs`

**Interfaces:**
- Consumes: `StateUpdate::System` and `SourceHealth` from Task 1.
- Produces: `CalendarRecord`, `parse_calendar_json`, `spawn_calendar_source`.

- [ ] **Step 1: Add a failing fake-backend shell test**

The test creates a fake `gcalcli`, sets `CALENDAR_BACKEND=gcalcli`, isolated
`XDG_CACHE_HOME`, and `NEXT_EVENT_NOW_EPOCH`, then asserts `--json` returns
`id`, `title`, `start_epoch`, `end_epoch`, `location`, and `healthy=true`. It
also asserts existing `--waybar` output still parses as JSON.

- [ ] **Step 2: Run the shell test and verify failure**

Run: `bash scripts/tests/next_event_test.sh`

Expected: FAIL because `--json` and deterministic backend selection do not yet
exist.

- [ ] **Step 3: Implement one structured record behind all output modes**

Add `--json`, `CALENDAR_BACKEND=auto|khal|gcalcli`, and
`NEXT_EVENT_NOW_EPOCH` for deterministic tests. Cache JSON containing:

```json
{"id":"gcalcli:1800000600:Design review","title":"Design review","location":"Room 2","start_epoch":1800000600,"end_epoch":1800002400,"healthy":true}
```

Derive the ID from backend, start time, and title when the backend lacks one.
Format text and Waybar responses from that record so current consumers retain
their behavior. Emit `healthy=false` with an error field only in JSON mode;
legacy modes stay quiet or show the existing empty behavior.

- [ ] **Step 4: Parse the record in Rust and publish health separately**

`parse_calendar_json(&str) -> Result<CalendarRecord>` must reject missing or
non-positive timestamps and an end before start. The source polls every 30
seconds, applies the script's cache behavior, and publishes `Disconnected`
without clearing a still-fresh prior event.

- [ ] **Step 5: Run shell and Rust tests**

Run: `bash scripts/tests/next_event_test.sh && cargo test --manifest-path tools/bar/Cargo.toml calendar`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/next_event.sh scripts/tests tools/bar/src/sources
git commit -m "feat(bar): expose structured calendar context"
```

### Task 5: Report Explicit Build And Test Activity From Zsh

**Files:**
- Create: `bar/shell-integration.zsh`
- Create: `bar/tests/shell-integration.zsh`
- Create: `tools/bar/src/activity.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `zshrc`

**Interfaces:**
- Consumes: `ControlRequest::ActivityStart/Finish` from Task 3.
- Produces: `classify_activity(command: &str) -> Option<&str>` in Zsh and `ActivityTracker::apply` in Rust.

- [ ] **Step 1: Write failing isolated Zsh tests**

Test that `cargo test -- --nocapture` becomes `Cargo test`, `pytest -k secret`
becomes `Pytest`, `git status` is ignored, and generated IPC arguments contain
the label and cwd but never the raw command or `secret`.

- [ ] **Step 2: Run the tests and verify failure**

Run: `zsh -f bar/tests/shell-integration.zsh`

Expected: FAIL because the integration file does not exist.

- [ ] **Step 3: Implement classification and hooks**

Use `autoload -Uz add-zsh-hook`, a `preexec` hook to classify only configured
prefixes, and a `precmd` hook that captures the previous exit status. Generate
an ID from shell PID plus an incrementing counter. Invoke `cockpit-bar activity
start/finish` asynchronously with stdio redirected; never pass raw command
arguments. Guard hook registration with `[[ -o interactive ]]` and
`command -v cockpit-bar`.

- [ ] **Step 4: Implement activity lifecycle state**

`ActivityTracker` ignores duplicate starts, completes only matching IDs, limits
retained completion cards, and expires completed work after the configured 30
seconds. Unknown finish IDs are logged once and ignored.

- [ ] **Step 5: Source the integration and run tests**

Append a guarded source of `~/.config/cockpit-bar/shell-integration.zsh` to
`zshrc`. Run:

`zsh -n zshrc bar/shell-integration.zsh && zsh -f bar/tests/shell-integration.zsh && cargo test --manifest-path tools/bar/Cargo.toml activity`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add bar zshrc tools/bar/src
git commit -m "feat(bar): surface explicit shell activity"
```

### Task 6: Normalize Hyprland And Niri State

**Files:**
- Create: `tools/bar/src/compositor/mod.rs`
- Create: `tools/bar/src/compositor/hyprland.rs`
- Create: `tools/bar/src/compositor/niri.rs`
- Create: `tools/bar/tests/fixtures/hyprland-events.txt`
- Create: `tools/bar/tests/fixtures/hyprland-snapshot.json`
- Create: `tools/bar/tests/fixtures/niri-events.jsonl`
- Modify: `tools/bar/src/lib.rs`

**Interfaces:**
- Consumes: normalized output/workspace/window types and `StateUpdate`.
- Produces: `CompositorAdapter` trait, `detect_compositor`, `HyprlandAdapter`, `NiriAdapter`.

- [ ] **Step 1: Add failing fixture tests**

Fixtures must cover two outputs, workspace activation, a title containing a
comma, urgent state, a moved window, focused-output change, and keyboard layout
change. Assert both adapters produce equivalent normalized transitions.

- [ ] **Step 2: Run the adapter tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml compositor`

Expected: FAIL because the adapters do not exist.

- [ ] **Step 3: Define the shared adapter contract**

```rust
pub trait CompositorAdapter: Send {
    fn initial_snapshot(&mut self) -> anyhow::Result<Vec<StateUpdate>>;
    fn next_update(&mut self) -> anyhow::Result<StateUpdate>;
    fn execute(&mut self, action: CompositorAction) -> anyhow::Result<()>;
}
```

Detection prefers `NIRI_SOCKET`, then `HYPRLAND_INSTANCE_SIGNATURE`, and fails
with a precise unsupported-session error instead of guessing from process
names.

- [ ] **Step 4: Implement event-driven adapters and resync**

Hyprland reads its event socket and snapshots `hyprctl -j monitors workspaces
clients devices`. Niri reads `niri msg --json event-stream` and snapshots the
matching JSON queries. Parsers must preserve full titles, map workspaces to
outputs, track each output's most recently focused window, and convert layout
changes into one normalized label. EOF or malformed input returns a reconnect
error; the supervisor reruns the initial snapshot before marking healthy.

- [ ] **Step 5: Implement compositor actions**

Support switch workspace, focus window, cycle workspace, and cycle keyboard
layout using `hyprctl dispatch/switchxkblayout` or `niri msg action`. Build
argument vectors directly; do not route titles or identifiers through `sh -c`.

- [ ] **Step 6: Run fixture and full tests**

Run: `cargo test --manifest-path tools/bar/Cargo.toml compositor && cargo test --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add tools/bar/src/compositor tools/bar/tests/fixtures tools/bar/src/lib.rs
git commit -m "feat(bar): normalize Hyprland and Niri context"
```

### Task 7: Add Power, Battery, CPU, Memory, And Clock Sources

**Files:**
- Create: `tools/bar/src/sources/mod.rs`
- Create: `tools/bar/src/sources/power.rs`
- Create: `tools/bar/src/sources/resources.rs`
- Modify: `tools/bar/src/lib.rs`

**Interfaces:**
- Consumes: `StateUpdate`, `SourceHealth`, threshold config.
- Produces: `SourceSupervisor`, `read_proc_sample`, `battery_severity`, `spawn_power_source`, `spawn_resource_source`, `spawn_clock_source`.

- [ ] **Step 1: Write failing parser and severity tests**

Use fixed `/proc/stat` and `/proc/meminfo` fixtures. Test first-sample behavior,
CPU delta calculation, available-memory percentage, charging suppression of
warnings, exact 15%/7% boundaries, and power-profile mapping for performance,
balanced, and power-saver.

- [ ] **Step 2: Run source tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml sources::power sources::resources`

Expected: FAIL with missing source modules.

- [ ] **Step 3: Implement source workers and supervision**

Each worker receives a cancellation flag and update sender. Power reads UPower
properties through blocking zbus and falls back to `/sys/class/power_supply`
for read-only battery state. The same worker reads Power Profiles D-Bus and
falls back to `powerprofilesctl get`; failure of power profiles does not hide a
healthy battery. Resources read `/proc` every five seconds. Clock publishes only
on minute boundaries. `SourceSupervisor` reconnects with delays 1, 2, 4, 8,
then 30 seconds and resets the delay after a healthy snapshot.

- [ ] **Step 4: Run focused and full tests**

Run: `cargo test --manifest-path tools/bar/Cargo.toml sources && cargo test --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tools/bar/src/sources tools/bar/src/lib.rs
git commit -m "feat(bar): monitor power and resource pressure"
```

### Task 8: Add Network, Bluetooth/Audio, And Media Sources

**Files:**
- Create: `tools/bar/src/sources/network.rs`
- Create: `tools/bar/src/sources/bluetooth.rs`
- Create: `tools/bar/src/sources/audio.rs`
- Create: `tools/bar/src/sources/media.rs`
- Modify: `tools/bar/src/sources/mod.rs`

**Interfaces:**
- Consumes: source supervision and system update types.
- Produces: `NetworkState`, `BluetoothState`, `AudioState`, `MediaState` publishers and parser functions.

- [ ] **Step 1: Write failing pure mapping tests**

Test NetworkManager connectivity values, Wi-Fi SSID/signal mapping, BlueZ
powered/connected-device mapping, muted and non-muted `wpctl` volume output,
`playerctl` metadata containing separators, playing-player preference, and
stopped-player removal.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml sources::network sources::bluetooth sources::media`

Expected: FAIL with missing modules.

- [ ] **Step 3: Implement event sources**

Use blocking zbus workers for NetworkManager and BlueZ property changes. Follow
`pactl subscribe` for audio changes and parse `wpctl get-volume
@DEFAULT_AUDIO_SINK@` for normalized volume and mute state. Use `playerctl
--follow metadata --format` as the MPRIS event bridge and restart it on EOF.
Reuse the configured headphones alias/MAC for the focused audio device, but
represent generic connected devices in the model. Publish disconnected health
when an optional service is absent; do not terminate the bar.

- [ ] **Step 4: Run source and full tests**

Run: `cargo test --manifest-path tools/bar/Cargo.toml sources && cargo test --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tools/bar/src/sources
git commit -m "feat(bar): monitor connectivity and media state"
```

### Task 9: Route Immediate Actions And Luma Workflows

**Files:**
- Create: `tools/bar/src/actions.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `tools/bar/src/model.rs`

**Interfaces:**
- Consumes: `ActionIntent`, `CompositorAction`, timer control operations.
- Produces: `ActionRouter<B: ActionBackend>::execute`, `ActionResult`, `ProcessSpec`.

- [ ] **Step 1: Write failing backend-spy tests**

Assert workspace clicks call the compositor backend, media scroll invokes
next/previous, power-profile scroll cycles a fixed order, title secondary-click
creates `Luma --query windows`, context secondary-click creates the card's
query, and no action uses a shell-expanded string.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml actions`

Expected: FAIL because `ActionRouter` does not exist.

- [ ] **Step 3: Implement typed routing**

Define backend methods for compositor, D-Bus, process, and timer operations.
Return `ActionResult::Failed { summary, detail }` to the originating popover.
Use direct argv for Luma, `playerctl`, `powerprofilesctl`, settings tools, and
existing Bluetooth/headphone scripts. Execute on a worker thread and publish
completion through the same UI update channel.

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path tools/bar/Cargo.toml actions && cargo test --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add tools/bar/src
git commit -m "feat(bar): route bar actions and Luma queries"
```

### Task 10: Render Primary And Reduced Layer-Shell Surfaces

**Files:**
- Create: `tools/bar/src/ui/mod.rs`
- Create: `tools/bar/src/ui/surface.rs`
- Create: `tools/bar/src/ui/wm.rs`
- Create: `tools/bar/src/ui/context_card.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `tools/bar/src/main.rs`

**Interfaces:**
- Consumes: immutable `BarSnapshot`, `select_context`, output roles, action intents.
- Produces: `BarApplication`, `SurfaceRegistry::reconcile`, `PrimarySurface`, `ReducedSurface`.

- [ ] **Step 1: Write failing presentation-model tests**

Keep GTK assertions out of policy tests. Test `surface_specs(snapshot, config)`
for one primary, reduced secondary outputs, focused fallback when `DP-5` is
absent, restoration when it returns, local workspace/title selection, and
critical-warning propagation to reduced bars.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ui::surface::tests`

Expected: FAIL because the UI modules do not exist.

- [ ] **Step 3: Implement surface reconciliation**

Create one undecorated non-resizable `ApplicationWindow` per `gdk::Monitor`, set
namespace `cockpit-bar`, layer `Top`, anchors top/left/right, exclusive zone 44,
and 5px top/side margins. Resolve monitor connectors through GTK and reconcile
connect/disconnect events without restarting the process.

- [ ] **Step 4: Build stable primary and reduced layouts**

Primary uses a three-column `Grid`: left and right size to content, center
expands in a bounded slot. Reduced uses left local WM context, an expanding
title, clock, and a hidden-unless-critical warning slot. Workspace buttons have
stable minimum sizes; titles ellipsize; card replacement uses GTK cross-fade
unless reduced motion is enabled. Clicking a title opens a compact GTK popover
of current windows grouped by output/workspace; selecting a row dispatches
`CompositorAction::FocusWindow`. Secondary-clicking the title emits the typed
Luma windows intent.

- [ ] **Step 5: Connect the state channel without blocking GTK**

Start source workers before `Application::run`, drain coalesced updates from a
GLib timeout, apply them to `StateStore`, and re-render only dirty components.
Start the control socket server and timer tick worker in the same application
lifecycle; cancel and join workers during shutdown.

- [ ] **Step 6: Run tests and a headless build**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ui && cargo build --manifest-path tools/bar/Cargo.toml`

Expected: PASS and a successful debug build without requiring a live display.

- [ ] **Step 7: Commit**

```bash
git add tools/bar/src
git commit -m "feat(bar): render primary and reduced surfaces"
```

### Task 11: Add System Modules And Mutually Exclusive Popovers

**Files:**
- Create: `tools/bar/src/ui/system.rs`
- Create: `tools/bar/src/ui/popovers.rs`
- Modify: `tools/bar/src/ui/mod.rs`
- Modify: `tools/bar/src/ui/surface.rs`

**Interfaces:**
- Consumes: system state, source health, `ActionIntent`, `ActionResult`.
- Produces: `SystemCluster`, `PopoverCoordinator`, module presentation functions.

- [ ] **Step 1: Write failing presentation tests**

Test labels/classes/tooltips for US/Dvorak layout, CPU/memory pressure, connected
and disconnected network, Bluetooth off, charging/low/critical battery, stale
state, media, and action failure. Test coordinator state so opening one popover
closes the previous one and Escape clears the active ID.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ui::system ui::popovers`

Expected: FAIL with missing modules.

- [ ] **Step 3: Implement the right cluster and focused popovers**

Create icon buttons for keyboard, resources, network, Bluetooth/audio, battery
and power profile, and clock. Use labels only where identity/severity needs
text. Popovers show immediate state and controls; their final row opens Luma or
the appropriate settings tool. Add click and conventional scroll controllers
for workspace, keyboard, media/volume, and power profile only.

- [ ] **Step 4: Implement error and health presentation**

Action failure stays inside the originating popover until the next action or
close. Stale state receives `.stale`; disconnected optional services receive
`.disconnected` and a dependency explanation. Do not emit desktop notifications
for repeating source errors.

- [ ] **Step 5: Run tests and build**

Run: `cargo test --manifest-path tools/bar/Cargo.toml ui && cargo build --manifest-path tools/bar/Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add tools/bar/src/ui
git commit -m "feat(bar): add interactive system popovers"
```

### Task 12: Add The Balanced-Glass Theme And Runtime Reload

**Files:**
- Create: `bar/style.css`
- Create: `matugen/templates/bar-colors.css`
- Modify: `matugen/config.toml`
- Create: `tools/bar/src/ui/theme.rs`
- Modify: `tools/bar/src/config.rs`
- Modify: `tools/bar/src/lib.rs`
- Modify: `matugen/UI_STYLE.md`

**Interfaces:**
- Consumes: semantic GTK classes and `AppConfig::load`.
- Produces: `load_css`, `reload_runtime_config`, SIGHUP reload behavior.

- [ ] **Step 1: Write failing path and reload tests**

Test XDG and home fallback paths, CSS composition order, successful threshold
reload, rejected structural reload preserving the old config, and primary-role
recalculation after a valid reload.

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --manifest-path tools/bar/Cargo.toml theme reload`

Expected: FAIL with missing theme loader/reload behavior.

- [ ] **Step 3: Implement tracked style and generated colors**

Use Cantarell with Symbols Nerd Font fallback, 44px bar height, 10px compact
controls, 14px controls, 18px shell, 1px outline-variant borders, 58% shell
opacity, 48-70% ordinary module opacity, and up to 82% warning/status opacity.
Map `.active` to primary, `.warning` to tertiary, and `.critical`, `.urgent`,
`.failed` to error. Keep all geometry in `bar/style.css`; generate Material
role definitions only in `bar/colors.css`.

- [ ] **Step 4: Implement CSS and SIGHUP reload**

Load stable CSS followed by generated colors through one `CssProvider`. On
SIGHUP, parse a new config first; only swap it after validation. Reconcile
output roles, module visibility, thresholds, and bindings. Log process-level
changes as restart-required without partially applying them.

- [ ] **Step 5: Render Matugen and verify**

Run: `~/.dotfiles/scripts/awww_wallpaper_watcher.sh --once` to reuse the tracked
wallpaper selection and Matugen invocation, then run
`cargo test --manifest-path tools/bar/Cargo.toml && cargo build --manifest-path tools/bar/Cargo.toml`.

Expected: `bar/colors.css` is generated locally, Rust tests pass, and the build
succeeds. Do not stage generated `bar/colors.css` if repository policy ignores
generated outputs.

- [ ] **Step 6: Commit**

```bash
git add bar/style.css matugen/templates/bar-colors.css matugen/config.toml matugen/UI_STYLE.md tools/bar/src
git commit -m "feat(bar): apply Matugen balanced-glass styling"
```

### Task 13: Install, Supervise, And Roll Out Behind A Waybar Fallback

**Files:**
- Create: `systemd/user/cockpit-bar.service`
- Modify: `setup.sh`
- Modify: `hypr/hyprland-config/startup.conf`
- Modify: `hypr/hyprland-config/base.conf`
- Modify: `niri/config.kdl`
- Create: `tools/bar/README.md`

**Interfaces:**
- Consumes: release `cockpit-bar`, config and CSS paths.
- Produces: repeatable installation, supervised startup, documented rollback.

- [ ] **Step 1: Add a failing integration contract check**

Add `tools/bar/tests/desktop_contract.sh` asserting setup links `bar` as
`~/.config/cockpit-bar`, the service uses the release binary and restart-on-
failure, both compositor configs start the service, both use namespace
`cockpit-bar`, and Waybar files remain present for rollback.

- [ ] **Step 2: Run the contract test to verify failure**

Run: `bash tools/bar/tests/desktop_contract.sh`

Expected: FAIL because installation and startup still reference Waybar.

- [ ] **Step 3: Add installation and service integration**

Link top-level `bar/` to `~/.config/cockpit-bar` in `setup.sh`; link the release
binary to `~/.local/bin/cockpit-bar` only when it exists. Define the service
with `Type=simple`, `ExecStart=%h/.local/bin/cockpit-bar`, `Restart=on-failure`,
`RestartSec=2`, and graphical-session environment. Make both compositors run
`systemctl --user restart cockpit-bar.service` after their existing environment
imports.

- [ ] **Step 4: Switch layer rules and preserve rollback instructions**

Add equivalent blur/shadow rules for namespace `cockpit-bar`. Comment the old
Waybar startup line instead of deleting Waybar configuration. Document exact
rollback commands and the prerequisite release build in `tools/bar/README.md`.

- [ ] **Step 5: Run static verification and build release**

Run:

```bash
bash tools/bar/tests/desktop_contract.sh
bash -n setup.sh scripts/*.sh waybar/scripts/*.sh hypr/scripts/modes/*.sh bspwm/modes/*.sh bspwm/helpers/*
zsh -n zshrc bar/shell-integration.zsh
cargo fmt --manifest-path tools/bar/Cargo.toml --check
cargo clippy --manifest-path tools/bar/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path tools/bar/Cargo.toml
cargo build --manifest-path tools/bar/Cargo.toml --release
niri validate -c niri/config.kdl
```

Expected: all commands exit zero.

- [ ] **Step 6: Install and smoke-test without removing fallback files**

Run `./setup.sh`, then `systemctl --user daemon-reload` and
`systemctl --user restart cockpit-bar.service`. Verify service logs contain one
detected compositor, one primary output, reduced secondary outputs, and no
reconnect loop.

- [ ] **Step 7: Commit**

```bash
git add systemd/user/cockpit-bar.service setup.sh hypr/hyprland-config/startup.conf hypr/hyprland-config/base.conf niri/config.kdl tools/bar/README.md tools/bar/tests
git commit -m "feat(bar): supervise Wayland command cockpit"
```

### Task 14: Perform Live Acceptance On Hyprland And Niri

**Files:**
- Modify if defects are found: only files owned by the failing component.
- Create: `docs/superpowers/verification/2026-07-08-wayland-command-cockpit-bar.md`

**Interfaces:**
- Consumes: complete bar and rollback path.
- Produces: acceptance evidence and defect-specific follow-up commits.

- [ ] **Step 1: Run the complete automated gate fresh**

Repeat formatting, clippy, Rust tests, shell syntax/tests, desktop contract, and
`niri validate`. Record command, exit code, and test count in the verification
document.

- [ ] **Step 2: Verify Hyprland behavior**

Exercise workspace switching, title picker, keyboard layout, each popover,
Luma routes, calendar threshold, timer completion, build success/failure,
network disconnect, and simulated low/critical discharging battery. Disconnect
and reconnect one optional service and confirm only its module degrades.

- [ ] **Step 3: Verify Niri behavior**

Repeat the same behavior checks under Niri, including output-local workspace and
window context and keyboard layout switching. Confirm the configured primary
returns after temporary output removal.

- [ ] **Step 4: Capture visual evidence**

Capture primary and reduced bars across DP-4, DP-5, and HDMI-A-2 for normal,
long-title, urgent-window, imminent-calendar, final-five-minute timer,
build-complete, disconnected-source, and low-battery states. Check text
ellipsizes, stable regions do not shift, no popover overlaps the bar, and the
glass treatment remains readable over the current wallpaper.

- [ ] **Step 5: Fix defects with focused red-green commits**

For each defect, first add the smallest reproducing test, run it red, implement
the fix, rerun it green, and use a specific message such as `fix(bar): preserve
window titles containing commas`. Do not combine unrelated compositor, source,
or styling defects.

- [ ] **Step 6: Record acceptance and final status**

Document compositor versions, output names, screenshots, simulations used, all
commands and outcomes, known optional dependencies, and the exact Waybar
rollback command. Run `git diff --check` and `git status --short` before the
final verification commit.

- [ ] **Step 7: Commit verification evidence**

```bash
git add docs/superpowers/verification/2026-07-08-wayland-command-cockpit-bar.md
git commit -m "test(bar): record Wayland cockpit acceptance"
```
