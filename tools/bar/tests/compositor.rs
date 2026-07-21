use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use cockpit_bar::{
    Direction, KeyboardLayoutOption, KeyboardLayoutState, OutputState, StateUpdate, SystemUpdate,
    WindowState, WorkspaceState,
    compositor::{
        CompositorAction, CompositorAdapter, HyprlandAdapter, NiriAdapter, detect_compositor,
    },
};

fn keyboard_state(current_index: u8) -> KeyboardLayoutState {
    let layouts = vec![
        KeyboardLayoutOption {
            index: 0,
            name: "English (US)".to_string(),
            layout: Some("us".to_string()),
            variant: None,
        },
        KeyboardLayoutOption {
            index: 1,
            name: "English (Dvorak)".to_string(),
            layout: Some("us".to_string()),
            variant: Some("dvorak".to_string()),
        },
        KeyboardLayoutOption {
            index: 2,
            name: "German (KOY)".to_string(),
            layout: Some("de".to_string()),
            variant: Some("koy".to_string()),
        },
    ];
    KeyboardLayoutState {
        current_index: Some(current_index),
        current_name: layouts
            .get(usize::from(current_index))
            .map(|layout| layout.name.clone()),
        layouts,
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"))
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod");
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => unsafe {
                std::env::set_var(self.key, value);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

fn expected_initial_snapshot() -> Vec<StateUpdate> {
    vec![
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::FocusedOutput(Some("DP-2".to_string())),
        StateUpdate::System(SystemUpdate::KeyboardLayout(keyboard_state(0))),
    ]
}

fn expected_transitions() -> Vec<StateUpdate> {
    vec![
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::FocusedOutput(Some("DP-1".to_string())),
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs".to_string(),
                        urgent: false,
                        workspace_id: Some("3".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs, Planning".to_string(),
                        urgent: false,
                        workspace_id: Some("3".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs, Planning".to_string(),
                        urgent: false,
                        workspace_id: Some("3".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "300".to_string(),
                    app_id: Some("firefox".to_string()),
                    title: "Docs, Planning".to_string(),
                    urgent: false,
                    workspace_id: Some("3".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![
                    WindowState {
                        id: "100".to_string(),
                        app_id: Some("kitty".to_string()),
                        title: "Terminal".to_string(),
                        urgent: false,
                        workspace_id: Some("1".to_string()),
                        changed_at: 0,
                    },
                    WindowState {
                        id: "300".to_string(),
                        app_id: Some("firefox".to_string()),
                        title: "Docs, Planning".to_string(),
                        urgent: false,
                        workspace_id: Some("3".to_string()),
                        changed_at: 0,
                    },
                ],
                focused_window: Some(WindowState {
                    id: "300".to_string(),
                    app_id: Some("firefox".to_string()),
                    title: "Docs, Planning".to_string(),
                    urgent: false,
                    workspace_id: Some("3".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: true,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: true,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: true,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: true,
                changed_at: 0,
            },
        ]),
        StateUpdate::System(SystemUpdate::KeyboardLayout(keyboard_state(2))),
    ]
}

fn collect_updates(adapter: &mut dyn CompositorAdapter, count: usize) -> Vec<StateUpdate> {
    (0..count)
        .map(|_| adapter.next_update().expect("next compositor update"))
        .collect()
}

#[test]
fn compositor_fixtures_produce_equivalent_normalized_updates() {
    let mut hyprland = HyprlandAdapter::new_for_test(
        include_str!("fixtures/hyprland-snapshot.json"),
        include_str!("fixtures/hyprland-events.txt"),
        |_, _, _| Ok(()),
    );
    let mut niri = NiriAdapter::new_for_test(
        include_str!("fixtures/niri-snapshot-outputs.json"),
        include_str!("fixtures/niri-snapshot-workspaces.json"),
        include_str!("fixtures/niri-snapshot-windows.json"),
        include_str!("fixtures/niri-snapshot-keyboard-layouts.json"),
        include_str!("fixtures/niri-events.jsonl"),
        |_, _, _| Ok(()),
    );

    let expected_initial = expected_initial_snapshot();
    assert_eq!(
        hyprland.initial_snapshot().expect("hyprland snapshot"),
        expected_initial
    );
    assert_eq!(
        niri.initial_snapshot().expect("niri snapshot"),
        expected_initial
    );

    let expected_updates = expected_transitions();
    assert_eq!(
        collect_updates(&mut hyprland, expected_updates.len()),
        expected_updates
    );
    assert_eq!(
        collect_updates(&mut niri, expected_updates.len()),
        expected_updates
    );
}

#[test]
fn compositor_hyprland_workspacev2_updates_same_output_activity() {
    let snapshot = r#"{
  "monitors": [
    {
      "id": 1,
      "name": "DP-1",
      "focused": true,
      "activeWorkspace": { "id": 1, "name": "1" },
      "specialWorkspace": { "id": 0, "name": "" },
      "lastWindow": "0x64"
    },
    {
      "id": 2,
      "name": "DP-2",
      "focused": false,
      "activeWorkspace": { "id": 2, "name": "2" },
      "specialWorkspace": { "id": 0, "name": "" },
      "lastWindow": "0xc8"
    }
  ],
  "workspaces": [
    { "id": 1, "name": "1", "monitor": "DP-1" },
    { "id": 3, "name": "3", "monitor": "DP-1" },
    { "id": 2, "name": "2", "monitor": "DP-2" }
  ],
  "clients": [
    {
      "address": "0x64",
      "workspace": { "id": 1, "name": "1" },
      "class": "kitty",
      "title": "Terminal",
      "urgent": false,
      "mapped": true
    },
    {
      "address": "0xc8",
      "workspace": { "id": 2, "name": "2" },
      "class": "org.signal.Signal",
      "title": "Signal",
      "urgent": false,
      "mapped": true
    }
  ],
  "devices": {
    "keyboards": [
      {
        "name": "at-translated-set-2-keyboard",
        "active_keymap": "us"
      }
    ]
  }
}"#;
    let mut adapter =
        HyprlandAdapter::new_for_test(snapshot, "workspacev2>>3,3\n", |_, _, _| Ok(()));

    adapter.initial_snapshot().expect("initial snapshot");

    assert_eq!(
        adapter.next_update().expect("workspace update"),
        StateUpdate::Outputs(vec![
            OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![
                    WorkspaceState {
                        id: "1".to_string(),
                        label: "1".to_string(),
                        output: "DP-1".to_string(),
                        active: false,
                        urgent: false,
                        changed_at: 0,
                    },
                    WorkspaceState {
                        id: "3".to_string(),
                        label: "3".to_string(),
                        output: "DP-1".to_string(),
                        active: true,
                        urgent: false,
                        changed_at: 0,
                    },
                ],
                windows: vec![WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                },],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
            OutputState {
                name: "DP-2".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "2".to_string(),
                    label: "2".to_string(),
                    output: "DP-2".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    workspace_id: Some("2".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ])
    );
}

#[test]
fn compositor_hyprland_clients_default_missing_urgent_to_false() {
    let snapshot = r#"{
  "monitors": [
    {
      "id": 1,
      "name": "DP-1",
      "focused": true,
      "activeWorkspace": { "id": 1, "name": "1" },
      "specialWorkspace": { "id": 0, "name": "" },
      "lastWindow": "0x64"
    }
  ],
  "workspaces": [
    { "id": 1, "name": "1", "monitor": "DP-1" }
  ],
  "clients": [
    {
      "address": "0x64",
      "workspace": { "id": 1, "name": "1" },
      "class": "kitty",
      "title": "Terminal",
      "mapped": true
    }
  ],
  "devices": { "keyboards": [] }
}"#;
    let mut adapter = HyprlandAdapter::new_for_test(snapshot, "", |_, _, _| Ok(()));

    assert_eq!(
        adapter.initial_snapshot().expect("hyprland snapshot"),
        vec![
            StateUpdate::Outputs(vec![OutputState {
                name: "DP-1".to_string(),
                workspaces: vec![WorkspaceState {
                    id: "1".to_string(),
                    label: "1".to_string(),
                    output: "DP-1".to_string(),
                    active: true,
                    urgent: false,
                    changed_at: 0,
                }],
                windows: vec![WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }],
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
                    workspace_id: Some("1".to_string()),
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            }]),
            StateUpdate::FocusedOutput(Some("DP-1".to_string())),
        ]
    );
}

#[test]
fn compositor_hyprland_actions_use_direct_argv() {
    let recorded = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
    let capture = recorded.clone();
    let mut adapter = HyprlandAdapter::new_for_test(
        include_str!("fixtures/hyprland-snapshot.json"),
        include_str!("fixtures/hyprland-events.txt"),
        move |program, args, _| {
            capture
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            Ok(())
        },
    );

    adapter
        .execute(CompositorAction::SwitchWorkspace {
            output: "DP-2".to_string(),
            workspace: "2".to_string(),
        })
        .expect("switch workspace");
    adapter
        .execute(CompositorAction::FocusWindow {
            output: "DP-1".to_string(),
            window_id: "0x12c".to_string(),
        })
        .expect("focus window");
    adapter
        .execute(CompositorAction::CycleWorkspace {
            output: "DP-1".to_string(),
            direction: Direction::Next,
        })
        .expect("cycle workspace");
    adapter
        .execute(CompositorAction::CycleKeyboardLayout)
        .expect("cycle keyboard layout");
    adapter
        .execute(CompositorAction::SelectKeyboardLayout { index: 1 })
        .expect("select keyboard layout");

    assert_eq!(
        recorded.lock().unwrap().as_slice(),
        &[
            (
                "hyprctl".to_string(),
                vec![
                    "dispatch".to_string(),
                    "focusmonitor".to_string(),
                    "DP-2".to_string()
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "dispatch".to_string(),
                    "workspace".to_string(),
                    "2".to_string()
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "dispatch".to_string(),
                    "focuswindow".to_string(),
                    "address:0x12c".to_string(),
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "dispatch".to_string(),
                    "focusmonitor".to_string(),
                    "DP-1".to_string()
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "dispatch".to_string(),
                    "workspace".to_string(),
                    "e+1".to_string()
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "switchxkblayout".to_string(),
                    "at-translated-set-2-keyboard".to_string(),
                    "next".to_string(),
                ],
            ),
            (
                "hyprctl".to_string(),
                vec![
                    "switchxkblayout".to_string(),
                    "at-translated-set-2-keyboard".to_string(),
                    "1".to_string(),
                ],
            ),
        ]
    );
}

#[test]
fn compositor_niri_from_env_binds_all_commands_to_selected_socket() {
    let _env_guard = env_lock().lock().unwrap();
    let temp_dir = unique_temp_dir("niri-socket");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let log_path = temp_dir.join("niri.log");
    let script_path = temp_dir.join("niri");
    let expected_socket = temp_dir.join("selected.sock");
    let wrong_socket = temp_dir.join("ambient.sock");
    write_script(
        &script_path,
        &format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
expected_socket={expected_socket:?}
log_path={log_path:?}
if [[ "${{NIRI_SOCKET:-}}" != "$expected_socket" ]]; then
  echo "wrong socket: ${{NIRI_SOCKET:-unset}}" >&2
  exit 91
fi
printf '%s|%s\n' "${{NIRI_SOCKET}}" "$*" >> "$log_path"
case "$*" in
  "msg --json outputs")
    printf '%s\n' '{outputs_json}'
    ;;
  "msg --json workspaces")
    printf '%s\n' '{workspaces_json}'
    ;;
  "msg --json windows")
    printf '%s\n' '{windows_json}'
    ;;
  "msg --json keyboard-layouts")
    printf '%s\n' '{keyboard_layouts_json}'
    ;;
  "msg --json event-stream")
    exit 0
    ;;
  "msg action focus-monitor DP-2")
    exit 0
    ;;
  "msg action focus-workspace 2")
    exit 0
    ;;
  *)
    echo "unexpected args: $*" >&2
    exit 92
    ;;
esac
"#,
            expected_socket = expected_socket.display(),
            log_path = log_path.display(),
            outputs_json = include_str!("fixtures/niri-snapshot-outputs.json")
                .trim()
                .replace('\'', "'\"'\"'"),
            workspaces_json = include_str!("fixtures/niri-snapshot-workspaces.json")
                .trim()
                .replace('\'', "'\"'\"'"),
            windows_json = include_str!("fixtures/niri-snapshot-windows.json")
                .trim()
                .replace('\'', "'\"'\"'"),
            keyboard_layouts_json = include_str!("fixtures/niri-snapshot-keyboard-layouts.json")
                .trim()
                .replace('\'', "'\"'\"'")
        ),
    );

    let original_path = std::env::var("PATH").unwrap_or_default();
    let _path_guard = EnvGuard::set("PATH", &format!("{}:{}", temp_dir.display(), original_path));
    let _socket_guard = EnvGuard::set("NIRI_SOCKET", &wrong_socket.display().to_string());

    let mut adapter = NiriAdapter::from_env(&expected_socket.display().to_string())
        .expect("from_env should use selected socket");
    adapter
        .execute(CompositorAction::SwitchWorkspace {
            output: "DP-2".to_string(),
            workspace: "2".to_string(),
        })
        .expect("switch workspace");
    drop(adapter);

    let log = fs::read_to_string(&log_path).expect("read fake niri log");
    for line in log.lines() {
        assert!(
            line.starts_with(&format!("{}|", expected_socket.display())),
            "all niri commands must use the selected socket, got: {line}"
        );
    }

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn compositor_niri_actions_use_direct_argv() {
    let recorded = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
    let capture = recorded.clone();
    let mut adapter = NiriAdapter::new_for_test(
        include_str!("fixtures/niri-snapshot-outputs.json"),
        include_str!("fixtures/niri-snapshot-workspaces.json"),
        include_str!("fixtures/niri-snapshot-windows.json"),
        include_str!("fixtures/niri-snapshot-keyboard-layouts.json"),
        include_str!("fixtures/niri-events.jsonl"),
        move |program, args, _| {
            capture
                .lock()
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            Ok(())
        },
    );

    adapter
        .execute(CompositorAction::SwitchWorkspace {
            output: "DP-2".to_string(),
            workspace: "2".to_string(),
        })
        .expect("switch workspace");
    adapter
        .execute(CompositorAction::FocusWindow {
            output: "DP-1".to_string(),
            window_id: "300".to_string(),
        })
        .expect("focus window");
    adapter
        .execute(CompositorAction::CycleWorkspace {
            output: "DP-1".to_string(),
            direction: Direction::Previous,
        })
        .expect("cycle workspace");
    adapter
        .execute(CompositorAction::CycleKeyboardLayout)
        .expect("cycle keyboard layout");
    adapter
        .execute(CompositorAction::SelectKeyboardLayout { index: 2 })
        .expect("select keyboard layout");

    assert_eq!(
        recorded.lock().unwrap().as_slice(),
        &[
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "focus-monitor".to_string(),
                    "DP-2".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "focus-workspace".to_string(),
                    "2".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "focus-window".to_string(),
                    "--id".to_string(),
                    "300".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "focus-monitor".to_string(),
                    "DP-1".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "focus-workspace-up".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "switch-layout".to_string(),
                    "next".to_string(),
                ],
            ),
            (
                "niri".to_string(),
                vec![
                    "msg".to_string(),
                    "action".to_string(),
                    "switch-layout".to_string(),
                    "2".to_string(),
                ],
            ),
        ]
    );
}

#[test]
fn compositor_hyprland_eof_requests_resync() {
    let mut adapter = HyprlandAdapter::new_for_test(
        include_str!("fixtures/hyprland-snapshot.json"),
        "",
        |_, _, _| Ok(()),
    );

    adapter.initial_snapshot().expect("initial snapshot");
    let error = adapter
        .next_update()
        .expect_err("EOF should request resync");
    assert!(error.to_string().contains("requires resync"));
    assert!(error.to_string().contains("EOF"));
}

#[test]
fn compositor_niri_malformed_event_requests_resync() {
    let mut adapter = NiriAdapter::new_for_test(
        include_str!("fixtures/niri-snapshot-outputs.json"),
        include_str!("fixtures/niri-snapshot-workspaces.json"),
        include_str!("fixtures/niri-snapshot-windows.json"),
        include_str!("fixtures/niri-snapshot-keyboard-layouts.json"),
        "not-json\n",
        |_, _, _| Ok(()),
    );

    adapter.initial_snapshot().expect("initial snapshot");
    let error = adapter
        .next_update()
        .expect_err("malformed event should request resync");
    assert!(error.to_string().contains("requires resync"));
    assert!(error.to_string().contains("not-json"));
}

#[test]
fn compositor_detect_rejects_unknown_sessions() {
    let error = match detect_compositor(&[]) {
        Ok(_) => panic!("unknown environment should fail"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("unsupported session")
            && error.contains("NIRI_SOCKET")
            && error.contains("HYPRLAND_INSTANCE_SIGNATURE")
    );
}
