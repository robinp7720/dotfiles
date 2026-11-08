//! A short-lived layer-shell curtain; never owns focus or session recovery.
use std::{
    io::Read,
    os::fd::AsRawFd,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn tv_transition_run() -> i32;
}

pub fn run() -> i32 {
    // GTK lives only in this helper process, separate from the Qt shell.
    unsafe { tv_transition_run() }
}

pub struct Transition(Child);

impl Transition {
    pub fn begin() -> Option<Self> {
        if std::env::var("TV_MODE_REDUCED_MOTION").is_ok_and(|value| value == "1") {
            return None;
        }
        let child = Command::new(std::env::current_exe().ok()?)
            .arg("transition")
            .env("GDK_BACKEND", "wayland")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let mut transition = Self(child);
        let stdout = transition.0.stdout.as_mut()?;
        let mut fd = libc::pollfd {
            fd: stdout.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // Cosmetic failure must never prevent entering TV mode.
        let mut ready = [0];
        if unsafe { libc::poll(&mut fd, 1, 1500) } > 0
            && stdout.read_exact(&mut ready).is_ok()
            && ready == [b'R']
        {
            Some(transition)
        } else {
            None
        }
    }

    pub fn reveal(mut self) {
        // EOF starts the reveal. Do not block Qt's render/event loop waiting for it.
        self.0.stdin.take();
        // Reap promptly rather than retaining a zombie for the whole TV session.
        std::thread::spawn(move || drop(self));
    }
}

impl Drop for Transition {
    fn drop(&mut self) {
        self.0.stdin.take();
        let deadline = Instant::now() + Duration::from_millis(600);
        while self.0.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
