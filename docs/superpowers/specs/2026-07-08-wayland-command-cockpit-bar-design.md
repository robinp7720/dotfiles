# Wayland Command Cockpit Bar Design

> Naming update (2026-07-23): the system is now **Vigil**. “Cockpit” remains
> the interaction and visual design language; historical implementation names
> in this document describe the original build.

## Goal

Replace Waybar on Hyprland and Niri with a responsive Rust GTK/layer-shell bar
that combines workspace and window context, prioritized activity, system state,
and direct actions. The bar must expose useful detail without becoming visually
dense, remain interactive while sources update, and degrade by module instead of
failing as a whole.

Polybar remains the legacy bar for bspwm/X11.

## Product Shape

The bar is a command cockpit with three stable regions on the primary output:

1. **WM context:** local workspaces, active application, window title, and urgent
   window state.
2. **Dynamic context:** one prioritized card representing the item that matters
   most now.
3. **System and actions:** keyboard layout, resource pressure, network,
   Bluetooth/audio, battery and power profile, and clock.

Secondary outputs use a reduced bar containing only output-local workspaces,
the local active window title, the clock, and critical warnings. They do not
duplicate the dynamic context or general system controls.

## Scope

The first release includes:

- Hyprland and Niri support.
- One full primary bar and reduced secondary bars.
- Workspace switching and focused-window context.
- Keyboard layout state and layout switching.
- CPU and memory pressure, network, Bluetooth/audio, battery, power profile,
  clock, media, calendar, timers, and build/test activity.
- Native GTK popovers for quick inspection and actions.
- Luma queries for deeper search, selection, and multi-step commands.
- Configurable module visibility, output roles, thresholds, and action bindings.
- Runtime recovery when an individual integration disconnects.

The first release does not include:

- Replacing Polybar for bspwm.
- A public third-party plugin ABI.
- A separate state daemon or cross-machine synchronization.
- Automatic classification of every foreground process.
- A full notification center, calendar client, network editor, or Bluetooth
  manager inside the bar.

## Repository Layout

- `tools/bar/` contains the Rust crate and the `cockpit-bar` executable.
- `bar/config.toml` contains user-facing behavior and output configuration.
- `bar/style.css` contains stable layout and component styling.
- `matugen/templates/bar-colors.css` is the color source used to generate
  `bar/colors.css`; generated colors are not edited directly.
- `scripts/next_event.sh` remains the shared calendar helper and gains a
  structured output mode while preserving its existing text and Waybar modes
  during migration.
- A small sourced Zsh integration reports selected long-running commands to the
  bar. The shell remains usable when the bar is absent.

The existing Waybar configuration and launcher stay available as a fallback
until both compositor paths pass live validation.

## Architecture

One `cockpit-bar` process runs per Wayland session and creates one GTK
layer-shell surface per active output. It detects the current compositor and
selects a matching adapter. The process is modular internally so state
collection, policy, rendering, and actions can be tested independently.

Only GTK rendering runs on the main thread. Event streams, D-Bus integrations,
filesystem reads, and subprocesses run asynchronously and publish typed updates
through channels. The UI receives coalesced state changes and updates only the
affected component.

### Session Adapter

The session adapter normalizes compositor-specific events into a shared model:

- Outputs and the currently focused output.
- Workspaces per output, including active and urgent state.
- Focused application identity and window title per output.
- Urgent windows.
- Keyboard layout and layout-switch actions.

Hyprland and Niri parsers are separate modules behind the same trait. Each
adapter prefers its compositor's event stream and uses snapshot queries only
for initial state and resynchronization.

### State Sources

Independent sources publish typed snapshots or events:

- UPower for battery and charging state, with sysfs as a read-only fallback.
- NetworkManager D-Bus for connectivity and active connection details.
- BlueZ and the existing audio tooling for device and headset state.
- Power Profiles D-Bus or `powerprofilesctl` for power mode.
- `/proc` for CPU and memory pressure.
- MPRIS for media metadata and playback actions.
- `scripts/next_event.sh --json` for a structured next-event timestamp, title,
  location, and source status.
- The bar's timer service for countdown state.
- A local activity endpoint for explicit build/test lifecycle events.

Sources must not expose raw command output to the UI. They parse external data
into domain types and attach a freshness timestamp and health state.

### State Store

The state store owns the latest normalized session, system, and activity state.
It deduplicates equivalent updates, expires stale values, and emits narrow
change notifications. Rendering code reads immutable snapshots and does not
query the operating system directly.

### Context Arbiter

The context arbiter is a pure policy module. Given current state, time,
configuration, and dismissal records, it selects at most one center card. It
does not render widgets or execute actions.

### Renderer

The renderer builds primary and reduced surfaces from shared GTK components.
Output role changes reuse the same process and rebuild only the affected
surface. Stable region dimensions prevent title, progress, or warning updates
from shifting unrelated controls.

### Action Router

The action router converts UI intents into compositor calls, D-Bus operations,
existing helper commands, timer operations, or Luma invocations. Actions are
asynchronous, cancellable where practical, and return a typed success or error
result to the originating component.

## Dynamic Context Policy

Eligible cards are grouped into four tiers:

1. **Critical:** critically low battery while discharging, timer completion,
   and urgent WM state.
2. **Imminent:** a calendar event starting soon, an active timer nearing
   completion, and actionable network or power warnings.
3. **Work:** a running or recently completed build/test command and active
   project context.
4. **Ambient:** active media and non-urgent calendar information.

The highest eligible tier wins. Within a tier, the item requiring action
soonest wins; equal urgency is resolved by most recent meaningful change.
Critical state interrupts immediately. When an override clears, the arbiter
re-evaluates all cards, allowing the previously eligible context to return.

Initial defaults are:

- Calendar events become imminent 15 minutes before their start.
- Timers become imminent during their final 5 minutes.
- Battery is low at 15% and critical at 7% while discharging.
- Completed work remains visible for 30 seconds unless replaced by a higher
  tier or dismissed.

All thresholds are configurable. A dismissed item remains suppressed until its
identity or severity changes. Critical battery and timer-complete cards cannot
be permanently dismissed while the condition remains active; dismissal only
snoozes their visual interruption for a short configured interval.

## Calendar, Timers, And Work Activity

Calendar priority requires machine-readable time. The calendar helper's JSON
mode returns an event identifier, title, optional location, start and end Unix
timestamps, and source health. Its cache stores the structured record rather
than only the formatted label. The existing text consumers format the same
record for backward compatibility. When a calendar backend does not expose an
identifier, the helper derives a stable identifier from the backend, start
time, and title.

Timers are owned by the bar and persisted under the user's XDG state directory
so a bar restart does not lose an active countdown. The executable exposes
timer subcommands for start, pause, resume, cancel, and list. Those subcommands
send requests to the running process over a user-only socket under
`XDG_RUNTIME_DIR`; they do not start a second bar instance. Luma presents those
commands as the deeper timer workflow, while the bar popover provides immediate
controls for current timers.

Build/test activity is explicit rather than inferred from arbitrary process
names. A sourced Zsh hook reports start and completion for a configurable
allowlist such as `cargo build`, `cargo test`, `npm test`, `pytest`, and project
run commands. Reports include command category, working directory, start time,
completion time, and exit status, but not unrestricted command output. The hook
communicates through the same user-only local socket and silently no-ops when
the bar is not running. It reports the configured activity label rather than
raw command arguments, preventing secrets passed on a command line from
entering bar state or logs.

## Interface And Interaction

### WM Context

- Clicking a workspace switches to it.
- Scrolling over workspaces moves between workspaces on that output.
- Clicking the window title opens a compact window picker.
- Secondary-clicking the title opens Luma with a window-focused query.
- Urgent state is visible in both the affected workspace and the dynamic card
  when it reaches the winning priority.

### Dynamic Card

The center shows one bounded card with an icon, concise label, optional progress
or remaining time, and at most one primary action. Clicking it opens a focused
GTK popover with immediate controls. Secondary click opens the corresponding
Luma query for deeper inspection or multi-step actions.

Card replacement uses a short cross-fade without moving the left or right
regions. Motion is disabled when GTK reduced-motion settings request it.

### System And Action Cluster

- Clicking an indicator opens its native GTK popover.
- Scrolling adjusts a continuous value or cycles a small ordered set only when
  that behavior is conventional: volume, workspace, keyboard layout, and power
  profile.
- Only one popover is open at a time. It closes on focus loss or `Escape`.
- Popovers provide immediate controls and status details, not full settings
  applications. A final action may open Luma or the appropriate settings tool.
- Icons use the installed symbolic icon theme; text appears where identity or
  severity cannot be communicated reliably by icon alone.

## Output Roles

The configured primary output receives the full bar. If it is absent, the
focused output becomes the temporary primary. When the configured output
returns, the full surface moves back without restarting the process.

Each secondary surface renders only:

- Workspaces belonging to that output.
- The focused window title for that output.
- Clock.
- Any critical warning that requires immediate awareness.

Workspace and window state must remain output-local even while the dynamic and
system state is session-global.

## Configuration And Theming

`bar/config.toml` defines:

- Preferred primary output and fallback behavior.
- Visible modules and their ordering.
- Calendar, timer, battery, and work-card thresholds.
- Command activity allowlist.
- Click and scroll bindings that differ from the defaults.
- Per-module stale-state and retry limits.

Structural configuration errors fail startup with a precise path and expected
type. Invalid optional values use documented defaults and emit one warning.
`SIGHUP` reloads presentation, thresholds, output roles, and bindings. Changes
that would alter process-level integration are logged as requiring restart.

Static styling remains separate from generated Matugen colors. CSS classes
represent semantic state such as `active`, `urgent`, `warning`, `critical`,
`stale`, and `disconnected`; source-specific code does not choose colors.

## Failure Handling

A failed source degrades only its module. The last valid value remains visible
only until its configured freshness deadline, after which the component shows
a subdued unavailable state or disappears when absence is less misleading.

Event streams and D-Bus sources reconnect with bounded exponential backoff and
resynchronize before reporting healthy. Repeated equivalent failures are
coalesced in logs and do not create repeated desktop notifications.

Actions never block rendering. A failed action produces a short inline error in
the relevant popover and a structured log entry containing the source and
attempted operation. Missing optional tools disable only their dependent action
and explain the dependency in the popover.

If the compositor connection is lost, surfaces remain present briefly with
stale WM state marked, then reconnect and rebuild output-local state. A fatal
GTK or configuration error exits non-zero so the user service can report and
restart it rather than leaving a silent partial process.

## Testing

Automated tests cover:

- Arbiter priority, threshold boundaries, time progression, dismissal,
  restoration, and simultaneous events.
- Hyprland and Niri parsing from recorded event fixtures.
- State deduplication, freshness expiry, and reconnect/resynchronization.
- Primary fallback and output-local workspace/window selection.
- Calendar JSON parsing and cache compatibility.
- Timer persistence and restart behavior.
- Zsh activity classification and no-op behavior without the socket.
- Action routing with mocked compositor, D-Bus, and process backends.

Development verification runs `cargo fmt --check`, `cargo clippy --all-targets
-- -D warnings`, and `cargo test` in `tools/bar`. Shell changes pass `bash -n`
or `zsh -n` as appropriate.

Live acceptance is performed under both Hyprland and Niri. Screenshots cover
the full primary bar and reduced secondary bars across the active three-monitor
layout, including long titles, an urgent workspace, imminent calendar and timer
cards, build completion, disconnected services, and simulated low battery.

## Acceptance Criteria

- All interactions remain responsive while slow sources refresh.
- Exactly one connected output has the full bar; all others have reduced bars.
- Workspace and focused-window context are correct for each output.
- Critical warnings preempt lower-priority context deterministically.
- Soon calendar events, timers, work state, and ambient media follow the defined
  ordering and return behavior.
- A source disconnect cannot crash or freeze the entire bar.
- Hyprland and Niri provide equivalent user-facing behavior through their
  adapters.
- Existing Waybar can be restored until the replacement completes live
  validation.
