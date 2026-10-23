# Config-only routine learning

Verified 2026-10-24 in an isolated nested Niri/Wayland session.

- `empty.png`: no suggestions; the entire Suggested now section is hidden.
- `live.png`: live suggestion with action and dismissal; no learning settings row.
- `routine.png`: learned routine with action and dismissal; no learning settings row.

An isolated running Vigil instance accepted `vigil routines clear` twice,
produced empty persisted observations and feedback, and omitted the legacy
learning flag. With that instance stopped, the command returned nonzero and
explained that the running service is required. User history was not cleared.

Rust tests cover both legacy flag values, config enable/disable and reload,
recording resumption, feedback/history clearing, persistence failure, preserved
live suggestions, CLI parsing, and IPC transport. Release build, strict Clippy,
and desktop/theme contracts passed. Nested-session D-Bus portal/systemd and accessibility-service startup
warnings were unrelated to Vigil's UI and command checks.
