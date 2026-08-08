use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::session;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlCommand {
    Show,
    Exit,
}

pub fn socket_path() -> Result<PathBuf> {
    Ok(session::runtime_dir()?.join("control.sock"))
}

pub fn spawn_server(
    sender: Sender<ControlCommand>,
    cancelled: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let path = socket_path()?;
    prepare_socket_path(&path)?;
    let listener = UnixListener::bind(&path)
        .with_context(|| format!("failed to bind TV mode socket {}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;

    Ok(thread::spawn(move || {
        while !cancelled.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = String::new();
                    if stream.read_to_string(&mut request).is_ok() {
                        let command = match request.trim() {
                            "show" => Some(ControlCommand::Show),
                            "exit" => Some(ControlCommand::Exit),
                            _ => None,
                        };
                        if let Some(command) = command {
                            let _ = sender.send(command);
                            let _ = stream.write_all(b"ok\n");
                        } else {
                            let _ = stream.write_all(b"error: unknown command\n");
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    eprintln!("TV mode control socket failed: {error}");
                    break;
                }
            }
        }
        let _ = fs::remove_file(&path);
    }))
}

pub fn send(command: ControlCommand, wait: Duration) -> Result<()> {
    let path = socket_path()?;
    let deadline = Instant::now() + wait;
    loop {
        match UnixStream::connect(&path) {
            Ok(mut stream) => {
                let request = match command {
                    ControlCommand::Show => "show\n",
                    ControlCommand::Exit => "exit\n",
                };
                stream.write_all(request.as_bytes())?;
                stream.shutdown(std::net::Shutdown::Write)?;
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                if response.starts_with("ok") {
                    return Ok(());
                }
                bail!("TV mode rejected control request: {}", response.trim());
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to connect to TV mode socket {}", path.display())
                });
            }
        }
    }
}

fn prepare_socket_path(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let current_uid = unsafe { libc::geteuid() };
    if metadata.uid() != current_uid {
        bail!(
            "refusing to replace foreign-owned socket {}",
            path.display()
        );
    }
    if !metadata.file_type().is_socket() {
        bail!("refusing to replace non-socket path {}", path.display());
    }
    fs::remove_file(path)
        .with_context(|| format!("failed to remove stale socket {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_socket_control_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("control.sock");
        fs::write(&path, "not a socket").unwrap();
        assert!(prepare_socket_path(&path).is_err());
        assert!(path.exists());
    }
}
