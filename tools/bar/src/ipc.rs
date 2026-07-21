use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::{ContextAction, ContextSnapshot, DesktopContext, TimerState};

const TRY_SERVE_READ_TIMEOUT: Duration = Duration::from_millis(50);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    TimerStart {
        label: String,
        duration_seconds: u64,
    },
    TimerPause {
        id: String,
    },
    TimerResume {
        id: String,
    },
    TimerCancel {
        id: String,
    },
    TimerList,
    ActivityStart {
        id: String,
        label: String,
        cwd: PathBuf,
        started_at: i64,
    },
    ActivityFinish {
        id: String,
        exit_code: i32,
        finished_at: i64,
    },
    ContextGet {
        context: Option<DesktopContext>,
    },
    ContextExecute {
        action: ContextAction,
    },
    ControlCenterOpen {
        context: DesktopContext,
        output: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResponse {
    Accepted,
    Timers {
        timers: Vec<TimerState>,
    },
    Contexts {
        contexts: Vec<ContextSnapshot>,
    },
    ActionResult {
        success: bool,
        message: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug)]
pub struct ControlClient {
    socket_path: PathBuf,
}

#[derive(Debug)]
pub struct ControlSocket {
    listener: UnixListener,
    socket_path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingPathKind {
    Socket,
    NonSocket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExistingPathAction {
    RemoveSocket,
    RejectNonSocket,
    RejectForeignOwner,
}

pub fn control_socket_path() -> Result<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("XDG_RUNTIME_DIR is required for cockpit-bar control socket"))?;
    Ok(PathBuf::from(runtime_dir).join("cockpit-bar.sock"))
}

impl ControlClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            socket_path: control_socket_path()?,
        })
    }

    pub fn with_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn send(&self, request: &ControlRequest) -> Result<ControlResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).with_context(|| {
            format!(
                "failed to connect to control socket {}",
                self.socket_path.display()
            )
        })?;
        write_json_line(&mut stream, request).context("failed to write control request")?;
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .context("failed to clone control socket stream")?,
        );
        read_json_line(&mut reader).context("failed to read control response")
    }
}

impl ControlSocket {
    pub fn bind() -> Result<Self> {
        Self::bind_at(control_socket_path()?)
    }

    pub fn bind_at(path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = path.as_ref().to_path_buf();
        prepare_socket_path(&socket_path)?;
        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("failed to bind {}", socket_path.display()))?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to chmod {}", socket_path.display()))?;
        Ok(Self {
            listener,
            socket_path,
        })
    }

    pub fn local_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        self.listener.set_nonblocking(nonblocking).with_context(|| {
            format!(
                "failed to update blocking mode for {}",
                self.socket_path.display()
            )
        })
    }

    pub fn serve_once<F>(&self, handler: F) -> Result<()>
    where
        F: FnOnce(ControlRequest) -> Result<ControlResponse>,
    {
        let (mut stream, _) = self
            .listener
            .accept()
            .context("failed to accept control connection")?;
        let request = {
            let mut reader = BufReader::new(
                stream
                    .try_clone()
                    .context("failed to clone accepted control stream")?,
            );
            read_json_line(&mut reader).context("failed to read control request")?
        };
        let response = match handler(request) {
            Ok(response) => response,
            Err(error) => ControlResponse::Error {
                message: error.to_string(),
            },
        };
        write_json_line(&mut stream, &response).context("failed to write control response")
    }

    pub fn try_serve_once<F>(&self, handler: F) -> Result<bool>
    where
        F: FnOnce(ControlRequest) -> Result<ControlResponse>,
    {
        let (mut stream, _) = match self.listener.accept() {
            Ok(accepted) => accepted,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(error).context("failed to accept control connection"),
        };
        stream
            .set_read_timeout(Some(TRY_SERVE_READ_TIMEOUT))
            .context("failed to set accepted control stream read timeout")?;
        let request = {
            let mut reader = BufReader::new(
                stream
                    .try_clone()
                    .context("failed to clone accepted control stream")?,
            );
            match try_read_json_line(&mut reader).context("failed to read control request")? {
                Some(request) => request,
                None => return Ok(false),
            }
        };
        let response = match handler(request) {
            Ok(response) => response,
            Err(error) => ControlResponse::Error {
                message: error.to_string(),
            },
        };
        write_json_line(&mut stream, &response).context("failed to write control response")?;
        Ok(true)
    }
}

fn prepare_socket_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => match existing_path_action(
            if metadata.file_type().is_socket() {
                ExistingPathKind::Socket
            } else {
                ExistingPathKind::NonSocket
            },
            metadata.uid(),
            current_uid(),
        ) {
            ExistingPathAction::RemoveSocket => fs::remove_file(path)
                .with_context(|| format!("failed to remove stale socket {}", path.display())),
            ExistingPathAction::RejectNonSocket => {
                bail!(
                    "existing control socket path is not a socket: {}",
                    path.display()
                )
            }
            ExistingPathAction::RejectForeignOwner => {
                bail!(
                    "existing control socket is owned by a different user: {}",
                    path.display()
                )
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to inspect control socket path {}", path.display())),
    }
}

fn existing_path_action(
    kind: ExistingPathKind,
    owner_uid: u32,
    current_uid: u32,
) -> ExistingPathAction {
    match kind {
        ExistingPathKind::NonSocket => ExistingPathAction::RejectNonSocket,
        ExistingPathKind::Socket if owner_uid == current_uid => ExistingPathAction::RemoveSocket,
        ExistingPathKind::Socket => ExistingPathAction::RejectForeignOwner,
    }
}

fn write_json_line<T: Serialize>(stream: &mut UnixStream, value: &T) -> Result<()> {
    serde_json::to_writer(&mut *stream, value).context("serialize control payload")?;
    stream
        .write_all(b"\n")
        .context("write control payload delimiter")?;
    stream.flush().context("flush control payload")
}

fn read_json_line<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> Result<T> {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .context("read control payload line")?;
    if read == 0 {
        bail!("control socket closed before sending a newline-delimited JSON payload");
    }
    serde_json::from_str(line.trim_end()).context("parse control payload")
}

fn try_read_json_line<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<Option<T>> {
    let mut line = String::new();
    let read = match reader.read_line(&mut line) {
        Ok(read) => read,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error).context("read control payload line"),
    };

    if read == 0 {
        bail!("control socket closed before sending a newline-delimited JSON payload");
    }
    if !line.ends_with('\n') {
        return Ok(None);
    }
    serde_json::from_str(line.trim_end())
        .map(Some)
        .context("parse control payload")
}

fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock, mpsc};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::{ContextAction, DesktopContext, TimerState};

    use super::{
        ControlClient, ControlRequest, ControlResponse, ControlSocket, ExistingPathAction,
        ExistingPathKind, control_socket_path, existing_path_action,
    };

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn control_protocol_round_trips_over_json_and_socket() {
        let request = ControlRequest::ActivityStart {
            id: "build-42".to_string(),
            label: "cargo test".to_string(),
            cwd: PathBuf::from("/tmp/project"),
            started_at: 1_800_000_000,
        };
        let serialized = serde_json::to_string(&request).expect("serialize request");
        let deserialized: ControlRequest =
            serde_json::from_str(&serialized).expect("deserialize request");
        assert_eq!(deserialized, request);

        let response = ControlResponse::Timers {
            timers: vec![TimerState {
                id: "focus".to_string(),
                label: "Focus".to_string(),
                remaining_seconds: 1_500,
                target_epoch: Some(1_800_001_500),
                completed: false,
                changed_at: 0,
            }],
        };
        let serialized = serde_json::to_string(&response).expect("serialize response");
        let deserialized: ControlResponse =
            serde_json::from_str(&serialized).expect("deserialize response");
        assert_eq!(deserialized, response);

        let runtime_dir = TempDir::new("ipc-roundtrip");
        let _guard = EnvVarGuard::set("XDG_RUNTIME_DIR", runtime_dir.path());
        let socket = ControlSocket::bind().expect("bind control socket");
        let socket_path = socket.local_path().to_path_buf();
        let server = thread::spawn(move || {
            socket
                .serve_once(|incoming| {
                    assert_eq!(incoming, ControlRequest::TimerList);
                    Ok(ControlResponse::Timers { timers: Vec::new() })
                })
                .expect("serve request");
        });

        let client = ControlClient::new().expect("create client");
        let response = client
            .send(&ControlRequest::TimerList)
            .expect("send request");
        server.join().expect("join server");

        assert_eq!(response, ControlResponse::Timers { timers: Vec::new() });
        assert_eq!(socket_path, runtime_dir.path().join("cockpit-bar.sock"));
    }

    #[test]
    fn context_protocol_uses_stable_tagged_json() {
        let request = ControlRequest::ContextExecute {
            action: ContextAction::SetVolumePercent { percent: 42 },
        };
        let json = serde_json::to_string(&request).expect("serialize context action");
        assert_eq!(
            json,
            r#"{"type":"context_execute","action":{"type":"set_volume_percent","percent":42}}"#
        );
        assert_eq!(
            serde_json::from_str::<ControlRequest>(
                r#"{"type":"control_center_open","context":"bluetooth","output":"DP-5"}"#,
            )
            .expect("deserialize page request"),
            ControlRequest::ControlCenterOpen {
                context: DesktopContext::Bluetooth,
                output: Some("DP-5".to_string()),
            }
        );
    }

    #[test]
    fn control_socket_path_uses_runtime_dir() {
        let runtime_dir = TempDir::new("ipc-runtime");
        let _guard = EnvVarGuard::set("XDG_RUNTIME_DIR", runtime_dir.path());

        assert_eq!(
            control_socket_path().expect("resolve socket path"),
            runtime_dir.path().join("cockpit-bar.sock")
        );
    }

    #[test]
    fn binding_socket_sets_user_only_permissions() {
        let runtime_dir = TempDir::new("ipc-perms");
        let _guard = EnvVarGuard::set("XDG_RUNTIME_DIR", runtime_dir.path());

        let socket = ControlSocket::bind().expect("bind socket");
        let metadata = fs::metadata(socket.local_path()).expect("stat socket");

        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn binding_rejects_non_socket_paths_without_deleting_them() {
        let runtime_dir = TempDir::new("ipc-nonsocket");
        let socket_path = runtime_dir.path().join("cockpit-bar.sock");
        fs::write(&socket_path, "occupied").expect("write regular file");

        let error = ControlSocket::bind_at(&socket_path).expect_err("reject regular file");
        assert!(
            error
                .to_string()
                .contains("existing control socket path is not a socket"),
            "unexpected error: {error:#}"
        );
        assert_eq!(
            fs::read_to_string(&socket_path).expect("read regular file"),
            "occupied"
        );
    }

    #[test]
    fn foreign_owned_socket_paths_are_rejected_without_removal() {
        assert_eq!(
            existing_path_action(ExistingPathKind::Socket, 4_242, 1_000),
            ExistingPathAction::RejectForeignOwner
        );
    }

    #[test]
    fn missing_runtime_dir_is_rejected() {
        let _guard = EnvVarGuard::unset("XDG_RUNTIME_DIR");
        let error = control_socket_path().expect_err("missing runtime dir should fail");

        assert!(
            error.to_string().contains("XDG_RUNTIME_DIR"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn same_user_socket_can_be_replaced() {
        assert_eq!(
            existing_path_action(ExistingPathKind::Socket, 1_000, 1_000),
            ExistingPathAction::RemoveSocket
        );
    }

    #[test]
    fn non_socket_paths_are_rejected_by_policy() {
        assert_eq!(
            existing_path_action(ExistingPathKind::NonSocket, 1_000, 1_000),
            ExistingPathAction::RejectNonSocket
        );
    }

    #[test]
    fn try_serve_once_does_not_block_on_stalled_client_without_newline() {
        let runtime_dir = TempDir::new("ipc-stalled-client");
        let socket_path = runtime_dir.path().join("cockpit-bar.sock");
        let socket = ControlSocket::bind_at(&socket_path).expect("bind socket");
        socket.set_nonblocking(true).expect("set nonblocking");

        let stalled_client = UnixStream::connect(&socket_path).expect("connect stalled client");
        let (result_tx, result_rx) = mpsc::channel();

        let server = thread::spawn(move || {
            let result = socket.try_serve_once(|_| Ok(ControlResponse::Accepted));
            result_tx.send(result).expect("send result");
        });

        let result = match result_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = stalled_client.shutdown(std::net::Shutdown::Both);
                server.join().expect("join stalled server");
                panic!("try_serve_once blocked on a connected client without a newline");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("stalled server disconnected before reporting a result");
            }
        };

        let _ = stalled_client.shutdown(std::net::Shutdown::Both);
        server.join().expect("join stalled server");

        assert!(!result.expect("nonblocking stalled serve result"));
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("cockpit-bar-{label}-{unique}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("lock env");
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key,
                previous,
                _lock: lock,
            }
        }

        fn unset(key: &'static str) -> Self {
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("lock env");
            let previous = std::env::var_os(key);
            unsafe {
                std::env::remove_var(key);
            }
            Self {
                key,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }
}
