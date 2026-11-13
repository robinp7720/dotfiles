use anyhow::{Result, ensure};
use greetd_ipc::{AuthMessageType, Request, Response, codec::SyncCodec};
use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};
use zeroize::Zeroize;

pub enum Action {
    Begin(String),
    Answer(String),
    Cancel,
    Launch { cmd: Vec<String>, env: Vec<String> },
}
#[derive(Debug, PartialEq)]
pub enum Event {
    Prompt { secret: bool, message: String },
    Notice(String),
    Ready,
    Started,
    Cancelled,
    Rejected(String),
    Failed(String),
}
pub struct Client {
    stream: UnixStream,
    active: bool,
    ready: bool,
    prompt: bool,
}
impl Client {
    pub fn new(stream: UnixStream) -> Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(120)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        Ok(Self {
            stream,
            active: false,
            ready: false,
            prompt: false,
        })
    }
    fn exchange(&mut self, mut request: Request) -> Result<Response> {
        let sent = request.write_to(&mut self.stream);
        if let Request::PostAuthMessageResponse {
            response: Some(ref mut secret),
        } = request
        {
            secret.zeroize();
        }
        sent?;
        Ok(Response::read_from(&mut self.stream)?)
    }
    fn reset(&mut self) -> Result<()> {
        self.ready = false;
        self.prompt = false;
        // greetd 0.10.3 keeps its configuring session after an authentication
        // error, even across socket connections. Its first cancel can report a
        // broken pipe from the exited PAM worker AFTER removing that session.
        // A second, idempotent cancel confirms that the daemon is clear.
        for _ in 0..2 {
            match self.exchange(Request::CancelSession)? {
                Response::Success => {
                    self.active = false;
                    return Ok(());
                }
                Response::Error { .. } => (),
                _ => anyhow::bail!("Unexpected authentication prompt during cancellation"),
            }
        }
        anyhow::bail!("Could not reset authentication")
    }
    pub fn perform(&mut self, action: Action, mut emit: impl FnMut(Event)) -> Result<()> {
        let request = match action {
            Action::Begin(user) => {
                ensure!(!user.trim().is_empty(), "Enter a username");
                // Also recover sessions left by a disconnected or older client.
                self.reset()?;
                self.active = true;
                self.ready = false;
                self.prompt = false;
                Request::CreateSession { username: user }
            }
            Action::Answer(mut answer) => {
                if !self.active || !self.prompt {
                    answer.zeroize();
                    anyhow::bail!("No authentication prompt is pending");
                }
                self.prompt = false;
                Request::PostAuthMessageResponse {
                    response: Some(answer),
                }
            }
            Action::Cancel => {
                self.reset()?;
                emit(Event::Cancelled);
                return Ok(());
            }
            Action::Launch { cmd, env } => {
                ensure!(self.active && self.ready, "Authentication is not complete");
                Request::StartSession { cmd, env }
            }
        };
        let launching = matches!(request, Request::StartSession { .. });
        let answering = matches!(request, Request::PostAuthMessageResponse { .. });
        let mut response = self.exchange(request)?;
        loop {
            match response {
                Response::Success => {
                    self.prompt = false;
                    if launching {
                        self.active = false;
                        self.ready = false;
                        emit(Event::Started);
                    } else {
                        self.ready = true;
                        emit(Event::Ready);
                    }
                    return Ok(());
                }
                Response::Error {
                    error_type,
                    description,
                } => {
                    self.reset()?;
                    // Only a rejected answer opens a fresh prompt automatically.
                    // Creation/transport failures require an explicit retry.
                    if answering && matches!(error_type, greetd_ipc::ErrorType::AuthError) {
                        emit(Event::Rejected(description));
                    } else {
                        emit(Event::Failed(description));
                    }
                    return Ok(());
                }
                Response::AuthMessage {
                    auth_message_type,
                    auth_message,
                } => {
                    ensure!(!launching, "Unexpected authentication prompt");
                    match auth_message_type {
                        AuthMessageType::Secret | AuthMessageType::Visible => {
                            self.prompt = true;
                            emit(Event::Prompt {
                                secret: matches!(auth_message_type, AuthMessageType::Secret),
                                message: auth_message,
                            });
                            return Ok(());
                        }
                        AuthMessageType::Info | AuthMessageType::Error => {
                            emit(Event::Notice(auth_message));
                            response =
                                self.exchange(Request::PostAuthMessageResponse { response: None })?;
                        }
                    }
                }
            }
        }
    }
}

pub fn worker(
    socket: Option<PathBuf>,
    preview: bool,
) -> (mpsc::Sender<Action>, async_channel::Receiver<Event>) {
    let (send, receive) = mpsc::channel();
    let (events, stream) = async_channel::unbounded();
    std::thread::spawn(move || {
        let mut client = None;
        let mut preview_step = 0;
        while let Ok(action) = receive.recv() {
            let emit = |event| {
                let _ = events.send_blocking(event);
            };
            if preview {
                match action {
                    Action::Begin(_) => {
                        preview_step = 0;
                        emit(Event::Prompt {
                            secret: true,
                            message: "Password".into(),
                        });
                    }
                    Action::Answer(mut text) => {
                        if text == "wrong" {
                            emit(Event::Rejected(
                                "Authentication failed. Please try again.".into(),
                            ));
                        } else if text == "disconnect" {
                            emit(Event::Failed(
                                "Authentication service disconnected. Please try again.".into(),
                            ));
                        } else if text == "otp" && preview_step == 0 {
                            preview_step = 1;
                            emit(Event::Prompt {
                                secret: false,
                                message: "Verification code".into(),
                            });
                        } else {
                            emit(Event::Ready);
                        }
                        text.zeroize();
                    }
                    Action::Cancel => emit(Event::Cancelled),
                    Action::Launch { .. } => emit(Event::Started),
                }
                continue;
            }
            if client.is_none() {
                let connection = socket
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("GREETD_SOCK is not set"))
                    .and_then(|path| Client::new(UnixStream::connect(path)?));
                match connection {
                    Ok(connected) => client = Some(connected),
                    Err(_) => {
                        if let Action::Answer(mut secret) = action {
                            secret.zeroize();
                        }
                        emit(Event::Failed(
                            "Cannot connect to the authentication service. Try again.".into(),
                        ));
                        continue;
                    }
                }
            }
            if let Err(error) = client.as_mut().unwrap().perform(action, emit) {
                // Never format requests or errors that might contain submitted credentials.
                eprintln!(
                    "Authentication transport/protocol failure: {}",
                    error.root_cause()
                );
                client = None;
                let _ = events.send_blocking(Event::Failed(
                    "Authentication service unavailable. Try again.".into(),
                ));
            }
        }
    });
    (send, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn conversation(
        script: impl FnOnce(UnixStream) + Send + 'static,
        run: impl FnOnce(&mut Client),
    ) {
        let (a, b) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut b = b;
            b.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            assert!(matches!(receive(&mut b), Request::CancelSession));
            reply(&mut b, Response::Success);
            script(b)
        });
        run(&mut Client::new(a).unwrap());
        server.join().unwrap();
    }
    fn receive(s: &mut UnixStream) -> Request {
        Request::read_from(s).unwrap()
    }
    fn reply(s: &mut UnixStream, response: Response) {
        response.write_to(s).unwrap();
    }
    fn prompt(s: &mut UnixStream, kind: AuthMessageType, text: &str) {
        reply(
            s,
            Response::AuthMessage {
                auth_message_type: kind,
                auth_message: text.into(),
            },
        );
    }
    #[test]
    fn multi_step_pam_and_acknowledged_launch() {
        conversation(
            |mut s| {
                assert!(
                    matches!(receive(&mut s), Request::CreateSession { username } if username == "test")
                );
                prompt(&mut s, AuthMessageType::Info, "Welcome");
                assert!(matches!(
                    receive(&mut s),
                    Request::PostAuthMessageResponse { response: None }
                ));
                prompt(&mut s, AuthMessageType::Secret, "Password");
                assert!(
                    matches!(receive(&mut s), Request::PostAuthMessageResponse { response: Some(r) } if r == "secret")
                );
                prompt(&mut s, AuthMessageType::Visible, "Code");
                assert!(
                    matches!(receive(&mut s), Request::PostAuthMessageResponse { response: Some(r) } if r == "123")
                );
                reply(&mut s, Response::Success);
                assert!(
                    matches!(receive(&mut s), Request::StartSession { cmd, .. } if cmd == ["niri-session"])
                );
                reply(&mut s, Response::Success);
            },
            |c| {
                let mut events = vec![];
                c.perform(Action::Begin("test".into()), |e| events.push(e))
                    .unwrap();
                assert_eq!(
                    events,
                    [
                        Event::Notice("Welcome".into()),
                        Event::Prompt {
                            secret: true,
                            message: "Password".into()
                        }
                    ]
                );
                c.perform(Action::Answer("secret".into()), |e| events.push(e))
                    .unwrap();
                assert_eq!(
                    events.last(),
                    Some(&Event::Prompt {
                        secret: false,
                        message: "Code".into()
                    })
                );
                c.perform(Action::Answer("123".into()), |e| events.push(e))
                    .unwrap();
                assert_eq!(events.last(), Some(&Event::Ready));
                c.perform(
                    Action::Launch {
                        cmd: vec!["niri-session".into()],
                        env: vec![],
                    },
                    |e| events.push(e),
                )
                .unwrap();
                assert_eq!(events.last(), Some(&Event::Started));
            },
        );
    }
    #[test]
    fn rejected_password_resets_daemon_before_retry() {
        conversation(
            |mut s| {
                assert!(matches!(receive(&mut s), Request::CreateSession { .. }));
                prompt(&mut s, AuthMessageType::Secret, "Password");
                assert!(matches!(
                    receive(&mut s),
                    Request::PostAuthMessageResponse { .. }
                ));
                reply(
                    &mut s,
                    Response::Error {
                        error_type: greetd_ipc::ErrorType::AuthError,
                        description: "Authentication failed".into(),
                    },
                );
                // Like greetd 0.10.3, retain the configuring session until cancelled.
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
                assert!(matches!(receive(&mut s), Request::CreateSession { .. }));
                prompt(&mut s, AuthMessageType::Secret, "Password");
                assert!(matches!(
                    receive(&mut s),
                    Request::PostAuthMessageResponse { .. }
                ));
                reply(&mut s, Response::Success);
            },
            |c| {
                c.perform(Action::Begin("test".into()), |_| {}).unwrap();
                c.perform(Action::Answer("wrong".into()), |e| {
                    assert_eq!(e, Event::Rejected("Authentication failed".into()))
                })
                .unwrap();
                c.perform(Action::Begin("test".into()), |_| {}).unwrap();
                c.perform(Action::Answer("correct".into()), |e| {
                    assert_eq!(e, Event::Ready)
                })
                .unwrap();
            },
        );
    }
    #[test]
    fn reset_confirms_cleanup_after_worker_exit_and_bounds_errors() {
        for succeeds in [false, true] {
            conversation(
                move |mut s| {
                    assert!(matches!(receive(&mut s), Request::CreateSession { .. }));
                    prompt(&mut s, AuthMessageType::Secret, "Password");
                    for second in [false, true] {
                        assert!(matches!(receive(&mut s), Request::CancelSession));
                        reply(
                            &mut s,
                            if second && succeeds {
                                Response::Success
                            } else {
                                Response::Error {
                                    error_type: greetd_ipc::ErrorType::Error,
                                    description: "Broken pipe".into(),
                                }
                            },
                        );
                    }
                },
                |c| {
                    c.perform(Action::Begin("test".into()), |_| {}).unwrap();
                    let mut events = vec![];
                    let result = c.perform(Action::Cancel, |e| events.push(e));
                    assert_eq!(result.is_ok(), succeeds);
                    assert!(!c.ready && !c.prompt);
                    assert_eq!(
                        events,
                        if succeeds {
                            vec![Event::Cancelled]
                        } else {
                            vec![]
                        }
                    );
                },
            );
        }
    }
    #[test]
    fn switching_user_cancels_and_errors_allow_retry() {
        conversation(
            |mut s| {
                receive(&mut s);
                prompt(&mut s, AuthMessageType::Secret, "Password");
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
                assert!(
                    matches!(receive(&mut s),Request::CreateSession {username} if username == "other")
                );
                reply(
                    &mut s,
                    Response::Error {
                        error_type: greetd_ipc::ErrorType::AuthError,
                        description: "Denied".into(),
                    },
                );
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
                assert!(matches!(receive(&mut s), Request::CreateSession { .. }));
                prompt(&mut s, AuthMessageType::Secret, "Password");
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
            },
            |c| {
                c.perform(Action::Begin("first".into()), |_| {}).unwrap();
                c.perform(Action::Begin("other".into()), |e| {
                    assert_eq!(e, Event::Failed("Denied".into()))
                })
                .unwrap();
                c.perform(Action::Begin("first".into()), |_| {}).unwrap();
                c.perform(Action::Cancel, |e| assert_eq!(e, Event::Cancelled))
                    .unwrap();
                assert!(!c.active);
            },
        );
    }
    #[test]
    fn rejected_launch_is_cancelled_and_disconnect_never_succeeds() {
        conversation(
            |mut s| {
                receive(&mut s);
                reply(&mut s, Response::Success);
                receive(&mut s);
                reply(
                    &mut s,
                    Response::Error {
                        error_type: greetd_ipc::ErrorType::Error,
                        description: "Cannot start".into(),
                    },
                );
                assert!(matches!(receive(&mut s), Request::CancelSession));
                reply(&mut s, Response::Success);
            },
            |c| {
                c.perform(Action::Begin("test".into()), |_| {}).unwrap();
                c.perform(
                    Action::Launch {
                        cmd: vec![],
                        env: vec![],
                    },
                    |e| assert_eq!(e, Event::Failed("Cannot start".into())),
                )
                .unwrap();
            },
        );
        conversation(
            |s| {
                drop(s);
            },
            |c| {
                assert!(
                    c.perform(Action::Begin("test".into()), |_| panic!("must not succeed"))
                        .is_err()
                );
            },
        );
    }
    #[test]
    fn out_of_sequence_actions_are_rejected_without_io() {
        let (a, _b) = UnixStream::pair().unwrap();
        let mut client = Client::new(a).unwrap();
        assert!(
            client
                .perform(Action::Answer("secret".into()), |_| {})
                .is_err()
        );
        assert!(
            client
                .perform(
                    Action::Launch {
                        cmd: vec![],
                        env: vec![]
                    },
                    |_| {}
                )
                .is_err()
        );
    }
    #[test]
    fn preview_never_connects_and_exercises_extra_prompts() {
        let (send, events) = worker(Some("/not/a/socket".into()), true);
        send.send(Action::Begin("test".into())).unwrap();
        assert!(matches!(
            events.recv_blocking().unwrap(),
            Event::Prompt { secret: true, .. }
        ));
        send.send(Action::Answer("otp".into())).unwrap();
        assert!(matches!(
            events.recv_blocking().unwrap(),
            Event::Prompt { secret: false, .. }
        ));
        send.send(Action::Answer("123".into())).unwrap();
        assert_eq!(events.recv_blocking().unwrap(), Event::Ready);
        send.send(Action::Launch {
            cmd: vec!["never-run".into()],
            env: vec![],
        })
        .unwrap();
        assert_eq!(events.recv_blocking().unwrap(), Event::Started);
    }
}
