use std::sync::{Arc, Mutex};

use cockpit_bar::{
    Direction, OutputState, StateUpdate, SystemUpdate, WindowState, WorkspaceState,
    compositor::{
        CompositorAction, CompositorAdapter, HyprlandAdapter, NiriAdapter, detect_compositor,
    },
};

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
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
                    changed_at: 0,
                }),
                urgent: false,
                changed_at: 0,
            },
        ]),
        StateUpdate::FocusedOutput(Some("DP-2".to_string())),
        StateUpdate::System(SystemUpdate::KeyboardLayout(Some("us".to_string()))),
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
                focused_window: Some(WindowState {
                    id: "100".to_string(),
                    app_id: Some("kitty".to_string()),
                    title: "Terminal".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "300".to_string(),
                    app_id: Some("firefox".to_string()),
                    title: "Docs, Planning".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "300".to_string(),
                    app_id: Some("firefox".to_string()),
                    title: "Docs, Planning".to_string(),
                    urgent: false,
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
                focused_window: Some(WindowState {
                    id: "200".to_string(),
                    app_id: Some("org.signal.Signal".to_string()),
                    title: "Signal".to_string(),
                    urgent: true,
                    changed_at: 0,
                }),
                urgent: true,
                changed_at: 0,
            },
        ]),
        StateUpdate::System(SystemUpdate::KeyboardLayout(Some("de".to_string()))),
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
        |_, _| Ok(()),
    );
    let mut niri = NiriAdapter::new_for_test(
        include_str!("fixtures/niri-snapshot-outputs.json"),
        include_str!("fixtures/niri-snapshot-workspaces.json"),
        include_str!("fixtures/niri-snapshot-windows.json"),
        include_str!("fixtures/niri-snapshot-keyboard-layouts.json"),
        include_str!("fixtures/niri-events.jsonl"),
        |_, _| Ok(()),
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
fn compositor_hyprland_actions_use_direct_argv() {
    let recorded = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
    let capture = recorded.clone();
    let mut adapter = HyprlandAdapter::new_for_test(
        include_str!("fixtures/hyprland-snapshot.json"),
        include_str!("fixtures/hyprland-events.txt"),
        move |program, args| {
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
        ]
    );
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
        move |program, args| {
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
        ]
    );
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
