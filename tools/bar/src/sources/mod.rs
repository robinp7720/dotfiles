pub mod audio;
pub mod bluetooth;
pub mod brightness;
pub mod calendar;
pub mod media;
pub mod network;
pub mod power;
pub mod resources;

use std::ffi::CStr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError, Sender},
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::{ClockState, StateUpdate, SystemUpdate};

pub use audio::spawn_audio_source;
pub use bluetooth::spawn_bluetooth_source;
pub use brightness::{parse_brightnessctl_output, spawn_brightness_source};
pub use calendar::{CalendarRecord, parse_calendar_json, spawn_calendar_source};
pub use media::spawn_media_source;
pub use network::spawn_network_source;
pub use power::{battery_severity, spawn_power_source};
pub use resources::{read_proc_sample, spawn_resource_source};

const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(30),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RetryState {
    index: usize,
}

impl RetryState {
    fn failure_delay(&mut self) -> Duration {
        let delay = RETRY_DELAYS[self.index];
        if self.index + 1 < RETRY_DELAYS.len() {
            self.index += 1;
        }
        delay
    }

    fn reset(&mut self) {
        self.index = 0;
    }
}

pub(crate) enum CancellableRecv<T> {
    Item(T),
    Cancelled,
    Disconnected,
}

pub(crate) fn recv_with_cancellation<T>(
    receiver: &Receiver<T>,
    cancelled: &Arc<AtomicBool>,
    poll_interval: Duration,
) -> CancellableRecv<T> {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return CancellableRecv::Cancelled;
        }

        match receiver.recv_timeout(poll_interval) {
            Ok(value) => return CancellableRecv::Item(value),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return CancellableRecv::Disconnected,
        }
    }
}

pub(crate) fn forward_blocking_iterator<I, T>(iterator: I) -> Receiver<Option<T>>
where
    I: Iterator<Item = T> + Send + 'static,
    T: Send + 'static,
{
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        for item in iterator {
            if sender.send(Some(item)).is_err() {
                return;
            }
        }

        let _ = sender.send(None);
    });

    receiver
}

pub struct SourceSupervisor;

impl SourceSupervisor {
    pub fn spawn<F>(
        cancelled: Arc<AtomicBool>,
        success_interval: Duration,
        mut worker: F,
    ) -> thread::JoinHandle<()>
    where
        F: FnMut() -> Result<bool> + Send + 'static,
    {
        thread::spawn(move || {
            let mut retry_state = RetryState::default();

            loop {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }

                match worker() {
                    Ok(healthy_snapshot) => {
                        if healthy_snapshot {
                            retry_state.reset();
                        }
                        if !sleep_with_cancellation(success_interval, &cancelled) {
                            break;
                        }
                    }
                    Err(_) => {
                        let delay = retry_state.failure_delay();
                        if !sleep_with_cancellation(delay, &cancelled) {
                            break;
                        }
                    }
                }
            }
        })
    }
}

pub fn spawn_clock_source(
    sender: Sender<StateUpdate>,
    cancelled: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            if !sleep_until_next_minute_boundary(&cancelled) {
                break;
            }

            let epoch_seconds = current_epoch_seconds();
            let update = StateUpdate::System(SystemUpdate::Clock(ClockState {
                epoch_seconds,
                label: format_clock_label(epoch_seconds),
            }));

            if sender.send(update).is_err() {
                cancelled.store(true, Ordering::Relaxed);
                break;
            }
        }
    })
}

pub(crate) fn sleep_with_cancellation(duration: Duration, cancelled: &Arc<AtomicBool>) -> bool {
    let mut elapsed = Duration::ZERO;
    while elapsed < duration {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }

        let remaining = duration.saturating_sub(elapsed);
        let step = remaining.min(CANCEL_POLL_INTERVAL);
        thread::sleep(step);
        elapsed += step;
    }

    !cancelled.load(Ordering::Relaxed)
}

fn sleep_until_next_minute_boundary(cancelled: &Arc<AtomicBool>) -> bool {
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration,
        Err(_) => Duration::ZERO,
    };
    let seconds = now.as_secs();
    let nanos = now.subsec_nanos();
    let remainder = seconds % 60;
    let wait = if remainder == 0 && nanos == 0 {
        Duration::ZERO
    } else {
        let remaining_seconds = 59 - remainder;
        Duration::from_secs(remaining_seconds)
            + Duration::from_nanos(u64::from(1_000_000_000 - nanos))
    };
    sleep_with_cancellation(wait, cancelled)
}

fn current_epoch_seconds() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time");
    i64::try_from(now.as_secs()).unwrap_or(i64::MAX)
}

fn format_clock_label(epoch_seconds: i64) -> String {
    let raw_time = match libc::time_t::try_from(epoch_seconds) {
        Ok(value) => value,
        Err(_) => return "00:00".to_string(),
    };

    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let mut buffer = [0_u8; 6];
    let format = b"%H:%M\0";

    // SAFETY: `raw_time` points to valid input, `tm` and `buffer` are allocated for libc to
    // populate, and the format string is NUL-terminated.
    let written = unsafe {
        if libc::localtime_r(&raw_time, tm.as_mut_ptr()).is_null() {
            return "00:00".to_string();
        }

        libc::strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            format.as_ptr().cast(),
            tm.as_ptr(),
        )
    };

    if written == 0 {
        return "00:00".to_string();
    }

    // SAFETY: `strftime` wrote a NUL-terminated C string into `buffer`.
    unsafe { CStr::from_ptr(buffer.as_ptr().cast()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{CancellableRecv, RETRY_DELAYS, RetryState, recv_with_cancellation};

    #[test]
    fn retry_state_uses_the_required_backoff_sequence() {
        let mut retry = RetryState::default();
        let delays = (0..6).map(|_| retry.failure_delay()).collect::<Vec<_>>();

        assert_eq!(
            delays,
            vec![
                RETRY_DELAYS[0],
                RETRY_DELAYS[1],
                RETRY_DELAYS[2],
                RETRY_DELAYS[3],
                RETRY_DELAYS[4],
                RETRY_DELAYS[4],
            ]
        );
    }

    #[test]
    fn retry_state_resets_after_a_healthy_snapshot() {
        let mut retry = RetryState::default();
        let _ = retry.failure_delay();
        let _ = retry.failure_delay();

        retry.reset();

        assert_eq!(retry.failure_delay(), RETRY_DELAYS[0]);
    }

    #[test]
    fn cancellable_receive_exits_promptly_while_idle_after_cancellation() {
        let (_sender, receiver) = mpsc::channel::<u8>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let started = Instant::now();

        let handle = thread::spawn(move || {
            recv_with_cancellation(&receiver, &worker_cancelled, Duration::from_millis(10))
        });

        thread::sleep(Duration::from_millis(25));
        cancelled.store(true, Ordering::Relaxed);

        assert!(matches!(handle.join().unwrap(), CancellableRecv::Cancelled));
        assert!(started.elapsed() < Duration::from_millis(250));
    }

    #[test]
    fn cancellable_receive_still_delivers_available_items() {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        sender.send(42_u8).unwrap();

        assert!(matches!(
            recv_with_cancellation(&receiver, &cancelled, Duration::from_millis(10)),
            CancellableRecv::Item(42)
        ));
    }
}
