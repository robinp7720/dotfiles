mod artwork;
pub mod context_card;
pub mod control_center;
pub mod popovers;
pub mod surface;
pub mod system;
pub mod theme;
pub mod wm;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender, TryRecvError},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use tracing::warn;

use crate::{
    ActionCompletion, ActionRequest, ActionResult, ActionRouter, ActivityStatus, ActivityTracker,
    ActivityUpdate, AppConfig, CommandActivity, ControlRequest, ControlResponse, ControlSocket,
    ReloadStatus, SourceHealth, SourceId, StateStore, StateUpdate, SystemActionBackend,
    SystemUpdate, TimerStore, detect_compositor, reload_runtime_config, spawn_action_worker,
    spawn_audio_source, spawn_bluetooth_source, spawn_brightness_source, spawn_clock_source,
    spawn_media_source, spawn_network_source, spawn_power_source, spawn_resource_source,
};

pub use surface::{PrimarySurface, ReducedSurface, SurfaceRegistry, surface_specs};

const UI_TICK_INTERVAL: Duration = Duration::from_millis(50);
const CONTROL_SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(100);
const TIMER_TICK_INTERVAL: Duration = Duration::from_secs(1);
const COMPOSITOR_RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const INITIAL_DRAIN_WINDOW: Duration = Duration::from_millis(150);
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

pub struct BarApplication {
    application: gtk::Application,
    runtime: RuntimeHandles,
}

struct RuntimeHandles {
    cancelled: Arc<AtomicBool>,
    joins: Vec<JoinHandle<()>>,
}

struct UiRuntime {
    config: AppConfig,
    config_path: PathBuf,
    store: StateStore,
    activity_tracker: ActivityTracker,
    registry: SurfaceRegistry,
    state_rx: Receiver<StateUpdate>,
    completion_rx: Receiver<ActionCompletion>,
    action_tx: Sender<ActionRequest>,
}

impl BarApplication {
    pub fn new(config: AppConfig, config_path: &Path) -> Result<Self> {
        install_sighup_handler()?;
        let (ui_runtime, runtime) = start_runtime(config, config_path)?;
        let application = gtk::Application::builder()
            .application_id("dev.robin.cockpit-bar")
            .build();

        let ui_runtime = Rc::new(RefCell::new(Some(ui_runtime)));
        let monitor_dirty = Rc::new(Cell::new(true));

        {
            let ui_runtime = Rc::clone(&ui_runtime);
            let monitor_dirty = Rc::clone(&monitor_dirty);
            application.connect_activate(move |application| {
                let Some(runtime) = ui_runtime.borrow_mut().take() else {
                    monitor_dirty.set(true);
                    return;
                };
                install_ui_loop(application, runtime, Rc::clone(&monitor_dirty));
            });
        }

        {
            let cancelled = Arc::clone(&runtime.cancelled);
            application.connect_shutdown(move |_| {
                cancelled.store(true, Ordering::Relaxed);
            });
        }

        Ok(Self {
            application,
            runtime,
        })
    }

    pub fn run(self) {
        self.application.run();
        self.runtime.shutdown();
    }
}

impl RuntimeHandles {
    fn shutdown(self) {
        self.cancelled.store(true, Ordering::Relaxed);
        for handle in self.joins {
            if let Err(error) = handle.join() {
                warn!("worker panicked during shutdown: {:?}", error);
            }
        }
    }
}

fn install_ui_loop(
    application: &gtk::Application,
    runtime: UiRuntime,
    monitor_dirty: Rc<Cell<bool>>,
) {
    let state = Rc::new(RefCell::new(runtime));
    let theme_provider = Rc::new(RefCell::new(None::<gtk::CssProvider>));

    if let Some(display) = gtk::gdk::Display::default() {
        if let Err(error) = refresh_theme_provider(
            &display,
            &state.borrow().config_path,
            &mut theme_provider.borrow_mut(),
        ) {
            warn!("theme css unavailable: {error:#}");
        }

        let monitors = display.monitors();
        let monitor_dirty_flag = Rc::clone(&monitor_dirty);
        monitors.connect_items_changed(move |_, _, _, _| {
            monitor_dirty_flag.set(true);
        });
    }

    {
        let monitor_dirty = Rc::clone(&monitor_dirty);
        let state = Rc::clone(&state);
        let application = application.clone();
        let theme_provider = Rc::clone(&theme_provider);
        glib::timeout_add_local(UI_TICK_INTERVAL, move || {
            let mut runtime = state.borrow_mut();
            let now_epoch = current_epoch();
            let mut dirty = drain_updates(&mut runtime, now_epoch);
            dirty |= handle_reload_request(
                &mut runtime,
                gtk::gdk::Display::default().as_ref(),
                &mut theme_provider.borrow_mut(),
                now_epoch,
            );
            dirty |= prune_completed_activities(&mut runtime, now_epoch);
            dirty |= runtime.store.expire(now_epoch);

            if dirty || monitor_dirty.replace(false) {
                let snapshot = runtime.store.snapshot().clone();
                let config = runtime.config.clone();
                let action_tx = runtime.action_tx.clone();
                runtime
                    .registry
                    .reconcile(&application, &snapshot, &config, &action_tx);
            }

            glib::ControlFlow::Continue
        });
    }

    let mut runtime = state.borrow_mut();
    let snapshot = runtime.store.snapshot().clone();
    let config = runtime.config.clone();
    let action_tx = runtime.action_tx.clone();
    runtime
        .registry
        .reconcile(application, &snapshot, &config, &action_tx);
}

fn drain_updates(runtime: &mut UiRuntime, now_epoch: i64) -> bool {
    let mut dirty = false;

    loop {
        match runtime.state_rx.try_recv() {
            Ok(StateUpdate::Activity(update)) => {
                if runtime.activity_tracker.apply(update, now_epoch) {
                    let activities = runtime
                        .activity_tracker
                        .snapshot()
                        .items
                        .into_values()
                        .collect::<Vec<_>>();
                    dirty |= runtime.store.apply(
                        StateUpdate::Activity(ActivityUpdate::Snapshot(activities)),
                        now_epoch,
                    );
                }
            }
            Ok(update) => {
                dirty |= runtime.store.apply(update, now_epoch);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    loop {
        match runtime.completion_rx.try_recv() {
            Ok(completion) => {
                dirty |= runtime.registry.handle_completion(&completion);
                if let ActionResult::Failed { detail, .. } = &completion.result {
                    warn!("action {} failed: {}", completion.origin, detail);
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    dirty
}

fn start_runtime(config: AppConfig, config_path: &Path) -> Result<(UiRuntime, RuntimeHandles)> {
    let (state_tx, state_rx) = mpsc::channel::<StateUpdate>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let timer_store = Arc::new(Mutex::new(TimerStore::load(current_epoch())?));
    let mut joins = vec![
        spawn_compositor_worker(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_resource_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_power_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_network_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_bluetooth_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_audio_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_brightness_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_media_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_clock_source(state_tx.clone(), Arc::clone(&cancelled)),
        spawn_timer_tick_worker(
            state_tx.clone(),
            Arc::clone(&cancelled),
            Arc::clone(&timer_store),
        ),
        spawn_control_socket_server(
            state_tx.clone(),
            Arc::clone(&cancelled),
            Arc::clone(&timer_store),
        )?,
    ];

    match resolve_calendar_script(config_path) {
        Some(calendar_script) => {
            joins.push(crate::spawn_calendar_source(
                calendar_script,
                state_tx.clone(),
                Arc::clone(&cancelled),
            ));
        }
        None => {
            let message = format!(
                "calendar helper unavailable for config {}",
                config_path.display()
            );
            warn!("{message}");
            let _ = state_tx.send(StateUpdate::Health {
                source: SourceId::Calendar,
                health: SourceHealth::Disconnected { message },
            });
        }
    }

    let mut store = StateStore::new(config.freshness.clone());
    let mut activity_tracker = ActivityTracker::new(config.thresholds.work_completed_seconds);
    seed_initial_state(&state_rx, &mut store, &mut activity_tracker);

    let router = ActionRouter::new(SystemActionBackend::from_env()?)
        .with_power_profile_state(store.snapshot().system.power.profile.clone());
    let (completion_tx, completion_rx) = mpsc::channel();
    let (action_tx, action_handle) =
        spawn_action_worker(router, completion_tx, Arc::clone(&cancelled));
    joins.push(action_handle);

    Ok((
        UiRuntime {
            config,
            config_path: config_path.to_path_buf(),
            store,
            activity_tracker,
            registry: SurfaceRegistry::default(),
            state_rx,
            completion_rx,
            action_tx,
        },
        RuntimeHandles { cancelled, joins },
    ))
}

fn seed_initial_state(
    state_rx: &Receiver<StateUpdate>,
    store: &mut StateStore,
    activity_tracker: &mut ActivityTracker,
) {
    let deadline = Instant::now() + INITIAL_DRAIN_WINDOW;

    loop {
        let now_epoch = current_epoch();
        match state_rx.try_recv() {
            Ok(StateUpdate::Activity(update)) => {
                if activity_tracker.apply(update, now_epoch) {
                    let activities = activity_tracker
                        .snapshot()
                        .items
                        .into_values()
                        .collect::<Vec<_>>();
                    let _ = store.apply(
                        StateUpdate::Activity(ActivityUpdate::Snapshot(activities)),
                        now_epoch,
                    );
                }
            }
            Ok(update) => {
                let _ = store.apply(update, now_epoch);
            }
            Err(TryRecvError::Empty) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
}

fn prune_completed_activities(runtime: &mut UiRuntime, now_epoch: i64) -> bool {
    if !runtime.activity_tracker.prune(now_epoch) {
        return false;
    }

    let activities = runtime
        .activity_tracker
        .snapshot()
        .items
        .into_values()
        .collect::<Vec<_>>();
    runtime.store.apply(
        StateUpdate::Activity(ActivityUpdate::Snapshot(activities)),
        now_epoch,
    )
}

fn spawn_compositor_worker(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !cancelled.load(Ordering::Relaxed) {
            let env = std::env::vars().collect::<Vec<_>>();
            let env_refs = env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect::<Vec<_>>();

            match detect_compositor(&env_refs) {
                Ok(mut compositor) => {
                    for update in compositor.initial_snapshot().unwrap_or_default() {
                        if sender.send(update).is_err() {
                            cancelled.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    let _ = sender.send(StateUpdate::Health {
                        source: SourceId::Compositor,
                        health: SourceHealth::Healthy,
                    });

                    loop {
                        if cancelled.load(Ordering::Relaxed) {
                            return;
                        }

                        match compositor.next_update_interruptibly(cancelled.as_ref()) {
                            Ok(Some(update)) => {
                                if sender.send(update).is_err() {
                                    cancelled.store(true, Ordering::Relaxed);
                                    return;
                                }
                            }
                            Ok(None) => return,
                            Err(error) => {
                                let _ = sender.send(StateUpdate::Health {
                                    source: SourceId::Compositor,
                                    health: SourceHealth::Disconnected {
                                        message: error.to_string(),
                                    },
                                });
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(StateUpdate::Health {
                        source: SourceId::Compositor,
                        health: SourceHealth::Disconnected {
                            message: error.to_string(),
                        },
                    });
                }
            }

            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            if !wait_for_cancellation(&cancelled, COMPOSITOR_RECONNECT_INTERVAL) {
                break;
            }
        }
    })
}

fn spawn_control_socket_server(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
    timer_store: Arc<Mutex<TimerStore>>,
) -> Result<JoinHandle<()>> {
    let socket = ControlSocket::bind()?;
    socket.set_nonblocking(true)?;

    Ok(thread::spawn(move || {
        while !cancelled.load(Ordering::Relaxed) {
            match socket
                .try_serve_once(|request| handle_control_request(request, &sender, &timer_store))
            {
                Ok(true) => {}
                Ok(false) => {
                    if !wait_for_cancellation(&cancelled, CONTROL_SOCKET_POLL_INTERVAL) {
                        break;
                    }
                }
                Err(error) => {
                    warn!("control socket error: {error:#}");
                    if !wait_for_cancellation(&cancelled, CONTROL_SOCKET_POLL_INTERVAL) {
                        break;
                    }
                }
            }
        }
    }))
}

fn handle_control_request(
    request: ControlRequest,
    sender: &Sender<StateUpdate>,
    timer_store: &Arc<Mutex<TimerStore>>,
) -> Result<ControlResponse> {
    match request {
        ControlRequest::TimerStart { .. }
        | ControlRequest::TimerPause { .. }
        | ControlRequest::TimerResume { .. }
        | ControlRequest::TimerCancel { .. }
        | ControlRequest::TimerList => {
            let now_epoch = current_epoch();
            let timers = timer_store
                .lock()
                .map_err(|_| anyhow!("timer store lock poisoned"))?
                .apply(&request, now_epoch)?;
            let _ = sender.send(StateUpdate::System(SystemUpdate::Timers(timers.clone())));
            Ok(ControlResponse::Timers { timers })
        }
        ControlRequest::ActivityStart {
            id,
            label,
            cwd,
            started_at,
        } => {
            let activity = CommandActivity {
                id,
                label,
                cwd,
                status: ActivityStatus::Running,
                started_at,
                finished_at: None,
                exit_code: None,
            };
            let _ = sender.send(StateUpdate::Activity(ActivityUpdate::Started(activity)));
            Ok(ControlResponse::Accepted)
        }
        ControlRequest::ActivityFinish {
            id,
            exit_code,
            finished_at,
        } => {
            let _ = sender.send(StateUpdate::Activity(ActivityUpdate::Finished {
                id,
                finished_at,
                exit_code,
            }));
            Ok(ControlResponse::Accepted)
        }
    }
}

fn spawn_timer_tick_worker(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
    timer_store: Arc<Mutex<TimerStore>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !cancelled.load(Ordering::Relaxed) {
            let now_epoch = current_epoch();
            match timer_store
                .lock()
                .map_err(|_| anyhow!("timer store lock poisoned"))
                .and_then(|mut store| store.snapshot(now_epoch))
            {
                Ok(timers) => {
                    if sender
                        .send(StateUpdate::System(SystemUpdate::Timers(timers)))
                        .is_err()
                    {
                        cancelled.store(true, Ordering::Relaxed);
                        return;
                    }
                }
                Err(error) => warn!("timer tick error: {error:#}"),
            }

            if !wait_for_cancellation(&cancelled, TIMER_TICK_INTERVAL) {
                break;
            }
        }
    })
}

fn resolve_calendar_script(config_path: &Path) -> Option<PathBuf> {
    let canonical_config = config_path.canonicalize().ok();
    let candidates = [
        canonical_config
            .as_deref()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(|root| root.join("scripts").join("next_event.sh")),
        canonical_config
            .as_deref()
            .and_then(Path::parent)
            .map(|parent| parent.join("scripts").join("next_event.sh")),
        config_path
            .parent()
            .map(|parent| parent.join("scripts").join("next_event.sh")),
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join("scripts").join("next_event.sh")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.is_file())
}

fn current_epoch() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time");
    i64::try_from(now.as_secs()).unwrap_or(i64::MAX)
}

extern "C" fn handle_sighup(_: libc::c_int) {
    RELOAD_REQUESTED.store(true, Ordering::Relaxed);
}

fn handle_reload_request(
    runtime: &mut UiRuntime,
    display: Option<&gtk::gdk::Display>,
    theme_provider: &mut Option<gtk::CssProvider>,
    now_epoch: i64,
) -> bool {
    if !RELOAD_REQUESTED.swap(false, Ordering::Relaxed) {
        return false;
    }

    match AppConfig::load(&runtime.config_path) {
        Ok(next) => match reload_runtime_config(&runtime.config, next) {
            crate::RuntimeConfigReload {
                config,
                status: ReloadStatus::Applied,
            } => {
                let threshold_changed = runtime.config.thresholds.work_completed_seconds
                    != config.thresholds.work_completed_seconds;
                runtime.config = config;
                if threshold_changed {
                    let activities = runtime
                        .activity_tracker
                        .snapshot()
                        .items
                        .into_values()
                        .collect::<Vec<_>>();
                    let mut tracker =
                        ActivityTracker::new(runtime.config.thresholds.work_completed_seconds);
                    let _ = tracker.apply(ActivityUpdate::Snapshot(activities), now_epoch);
                    runtime.activity_tracker = tracker;
                }
                if let Some(display) = display
                    && let Err(error) =
                        refresh_theme_provider(display, &runtime.config_path, theme_provider)
                {
                    warn!("theme reload failed: {error:#}");
                }
                true
            }
            crate::RuntimeConfigReload {
                status: ReloadStatus::RestartRequired { reasons },
                ..
            } => {
                warn!("config reload requires restart: {}", reasons.join(", "));
                false
            }
        },
        Err(error) => {
            warn!("config reload rejected: {error:#}");
            false
        }
    }
}

fn refresh_theme_provider(
    display: &gtk::gdk::Display,
    config_path: &Path,
    theme_provider: &mut Option<gtk::CssProvider>,
) -> Result<()> {
    if let Some(provider) = theme_provider.as_ref() {
        theme::reload_css(provider, config_path)?;
        return Ok(());
    }

    *theme_provider = Some(theme::load_css(display, config_path)?);
    Ok(())
}

fn install_sighup_handler() -> Result<()> {
    let previous = unsafe {
        libc::signal(
            libc::SIGHUP,
            handle_sighup as *const () as libc::sighandler_t,
        )
    };
    if previous == libc::SIG_ERR {
        return Err(anyhow!("failed to install SIGHUP handler"));
    }

    Ok(())
}

fn wait_for_cancellation(cancelled: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(Duration::from_millis(25)));
    }
    !cancelled.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{RuntimeHandles, resolve_calendar_script};

    #[test]
    fn calendar_helper_resolves_through_installed_config_symlink() {
        let root = temp_dir("calendar-config-symlink");
        let repo = root.join("dotfiles");
        let bar = repo.join("bar");
        let scripts = repo.join("scripts");
        let config_home = root.join("config");
        fs::create_dir_all(&bar).unwrap();
        fs::create_dir_all(&scripts).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        fs::write(bar.join("config.toml"), "").unwrap();
        fs::write(scripts.join("next_event.sh"), "#!/usr/bin/env bash\n").unwrap();
        std::os::unix::fs::symlink(&bar, config_home.join("cockpit-bar")).unwrap();

        let resolved = resolve_calendar_script(&config_home.join("cockpit-bar/config.toml"))
            .expect("calendar helper");

        assert_eq!(
            resolved.canonicalize().unwrap(),
            scripts.join("next_event.sh").canonicalize().unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_joins_workers_that_finish_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::new(AtomicBool::new(false));
        let worker_finished_flag = Arc::clone(&worker_finished);
        let cancelled_flag = Arc::clone(&cancelled);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            if cancelled_flag.load(Ordering::Relaxed) {
                worker_finished_flag.store(true, Ordering::Relaxed);
            }
        });

        RuntimeHandles {
            cancelled,
            joins: vec![handle],
        }
        .shutdown();

        assert!(worker_finished.load(Ordering::Relaxed));
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"))
    }
}
