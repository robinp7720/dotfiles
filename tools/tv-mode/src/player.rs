//! One worker owns the decoder and managed session, serializing progress/PATCH/DELETE.
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::oblecto::{Api, ApiError, number};

#[derive(Clone, Copy)]
pub enum CommandKind {
    TogglePause,
    Seek(f64),
    Volume(f64),
    Audio,
    Subtitles,
    Quality,
    Skip,
    Stop,
}
pub enum Event {
    Ready,
    Interaction,
    Notice(String),
    Finished { ended: bool, error: Option<String> },
}
pub struct Handle {
    tx: Sender<CommandKind>,
    join: Option<JoinHandle<()>>,
}
impl Handle {
    pub fn send(&self, command: CommandKind) {
        let _ = self.tx.send(command);
    }
    pub fn finish(mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.tx.send(CommandKind::Stop);
    }
}
pub fn start(
    api: Api,
    options: Value,
    program: String,
    fullscreen: bool,
) -> (Handle, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let (events, receive) = mpsc::channel();
    let join = thread::spawn(move || {
        let result = play(api, options, &program, fullscreen, rx, &events);
        let event = match result {
            Ok(ended) => Event::Finished { ended, error: None },
            Err(error) => Event::Finished {
                ended: false,
                error: Some(error.to_string()),
            },
        };
        let _ = events.send(event);
    });
    (
        Handle {
            tx,
            join: Some(join),
        },
        receive,
    )
}

struct Decoder {
    child: Child,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    directory: PathBuf,
}
impl Decoder {
    fn spawn(program: &str, fullscreen: bool) -> Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .context("No desktop runtime directory")?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let directory = runtime.join(format!("tv-oblecto-{}-{nonce}", std::process::id()));
        fs::create_dir(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let socket = directory.join("mpv.sock");
        let result = (|| {
            let mut child = Command::new(program)
                .args(["--no-config", "--idle=yes", "--force-window=yes", "--keep-open=no", "--terminal=no", "--osc=yes", "--hwdec=auto-safe", "--sub-auto=no", "--audio-file-auto=no", "--ytdl=no", "--title=Oblecto · TV mode", "--wayland-app-id=tv-mode-oblecto"])
                .arg(if fullscreen { "--fullscreen=yes" } else { "--fullscreen=no" })
                .arg(format!("--input-ipc-server={}", socket.display()))
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
                .context("Could not start mpv. Install mpv or update oblecto.player in the TV mode config.")?;
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                    return Ok((child, stream));
                }
                if child.try_wait()?.is_some() {
                    bail!("mpv exited before opening its control socket");
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("mpv did not become ready");
                }
                thread::sleep(Duration::from_millis(40));
            }
        })();
        match result {
            Ok((child, stream)) => Ok(Self {
                child,
                reader: BufReader::new(stream.try_clone()?),
                writer: stream,
                directory,
            }),
            Err(error) => {
                let _ = fs::remove_dir_all(directory);
                Err(error)
            }
        }
    }
    fn command(&mut self, args: Value) -> Result<()> {
        serde_json::to_writer(&mut self.writer, &json!({"command": args}))?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }
    fn event(&mut self) -> Result<Option<Value>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => bail!("The player closed"),
            Ok(_) => Ok(serde_json::from_str(&line).ok()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(_) => bail!("Lost connection to the player"),
        }
    }
    fn load(&mut self, api: &Api, descriptor: &Value, paused: bool) -> Result<()> {
        // libavformat HLS seeking can choose the following keyframe. Read ahead
        // from an earlier demuxer position, then decode to the exact absolute time.
        let media = api.media_url(
            descriptor["mediaUrl"]
                .as_str()
                .context("Missing playback URL")?,
        )?;
        self.command(json!(["loadfile", media, "replace", -1, {"start": number(&descriptor["position"]).unwrap_or(0.0).to_string(), "pause": if paused {"yes"} else {"no"}, "hr-seek": "yes", "hr-seek-demuxer-offset": if descriptor["method"] == "direct" {"0"} else {"12"}}]))
    }
    fn current_position(&mut self) -> Option<f64> {
        serde_json::to_writer(
            &mut self.writer,
            &json!({"command":["get_property","time-pos"],"request_id":99}),
        )
        .ok()?;
        self.writer.write_all(b"\n").ok()?;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Some(event) = self.event().ok()?
                && event["request_id"] == 99
            {
                return number(&event["data"]);
            }
        }
        None
    }
    fn osd(&mut self, message: &str) {
        let _ = self.command(json!(["show-text", message, 3500]));
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.command(json!(["quit"]));
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn session_path(descriptor: &Value) -> Result<String> {
    let id = descriptor["sessionId"]
        .as_str()
        .context("Missing playback session ID")?;
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "Invalid playback session ID"
    );
    Ok(format!("playback/sessions/{id}"))
}
fn progress(
    api: &Api,
    descriptor: &Value,
    position: f64,
    paused: bool,
    buffering: bool,
    failed: bool,
) -> Result<()> {
    let duration = number(&descriptor["duration"])
        .filter(|n| *n > 0.0)
        .unwrap_or(f64::MAX);
    api.request("POST", &format!("{}/progress", session_path(descriptor)?), &[], Some(json!({"revision":descriptor["revision"], "position":position.clamp(0.0,duration), "paused":paused, "buffering":buffering, "failed":failed})))?;
    Ok(())
}
fn replace(
    api: &Api,
    decoder: &mut Decoder,
    descriptor: &mut Value,
    mut patch: Value,
    position: f64,
    paused: bool,
) -> Result<()> {
    progress(api, descriptor, position, paused, false, false)?;
    patch["revision"] = descriptor["revision"].clone();
    patch["position"] = json!(position);
    *descriptor = api.request("PATCH", &session_path(descriptor)?, &[], Some(patch))?;
    decoder.load(api, descriptor, paused)
}
fn cycle_track(descriptor: &Value, subtitle: bool) -> Value {
    let kind = if subtitle { "subtitle" } else { "audio" };
    let key = if subtitle {
        "subtitleStreamIndex"
    } else {
        "audioStreamIndex"
    };
    let mut indexes = vec![];
    if subtitle {
        indexes.push(Value::Null);
    }
    if let Some(tracks) = descriptor["tracks"].as_array() {
        indexes.extend(
            tracks
                .iter()
                .filter(|t| t["codec_type"] == kind)
                .filter_map(|t| t["index"].as_u64().map(|v| json!(v))),
        );
    }
    let current = &descriptor["selectedTracks"][key];
    let next = indexes
        .iter()
        .position(|v| v == current)
        .map(|i| (i + 1) % indexes.len().max(1))
        .unwrap_or(0);
    let value = indexes.get(next).cloned().unwrap_or(Value::Null);
    let mut patch = json!({key: value});
    if subtitle {
        patch["subtitleMode"] = json!(if patch[key].is_null() { "off" } else { "auto" });
    }
    patch
}
fn track_label(descriptor: &Value, subtitle: bool) -> String {
    let key = if subtitle {
        "subtitleStreamIndex"
    } else {
        "audioStreamIndex"
    };
    let selected = &descriptor["selectedTracks"][key];
    let label = descriptor["tracks"]
        .as_array()
        .and_then(|tracks| tracks.iter().find(|t| &t["index"] == selected))
        .map(|t| {
            t["tags"]["title"]
                .as_str()
                .or_else(|| t["tags"]["language"].as_str())
                .unwrap_or("Track")
        });
    format!(
        "{}: {}",
        if subtitle { "Subtitles" } else { "Audio" },
        label.unwrap_or("Off")
    )
}
fn play(
    api: Api,
    mut options: Value,
    program: &str,
    fullscreen: bool,
    commands: Receiver<CommandKind>,
    events: &Sender<Event>,
) -> Result<bool> {
    let mut descriptor = api.request("POST", "playback/sessions", &[], Some(options.clone()))?;
    let mut position = number(&descriptor["position"]).unwrap_or(0.0);
    let mut paused = false;
    let mut buffering = true;
    let mut final_failed = false;
    let result = (|| {
        // Creation can outlast a navigation/shutdown request. Dispose it without starting a decoder.
        if commands.try_iter().any(|c| matches!(c, CommandKind::Stop)) {
            return Ok(false);
        }
        let mut decoder = Decoder::spawn(program, fullscreen)?;
        for (index, property) in [(1, "time-pos"), (2, "pause"), (3, "paused-for-cache")] {
            decoder.command(json!(["observe_property", index, property]))?;
        }
        for (key, action) in [
            ("ESC", "stop"),
            ("SPACE", "pause"),
            ("LEFT", "back"),
            ("RIGHT", "forward"),
            ("x", "subtitles"),
            ("y", "audio"),
            ("q", "quality"),
            ("n", "skip"),
        ] {
            decoder.command(json!([
                "keybind",
                key,
                format!("script-message tv-mode-oblecto {action}")
            ]))?;
        }
        decoder.load(&api, &descriptor, false)?;
        let _ = events.send(Event::Ready);
        let mut last_report = Instant::now();
        let mut report_after: Option<Instant> = None;
        let mut fallback = false;
        let mut recovered = false;
        let mut last_warning = String::new();
        let mut quality = 0;
        let mut keyboard_commands = VecDeque::new();
        loop {
            for command in commands.try_iter().chain(keyboard_commands.drain(..)) {
                let _ = events.send(Event::Interaction);
                match command {
                    CommandKind::Stop => {
                        position = decoder.current_position().unwrap_or(position);
                        return Ok(false);
                    }
                    CommandKind::TogglePause => {
                        decoder.command(json!(["cycle", "pause"]))?;
                    }
                    CommandKind::Seek(seconds) => {
                        decoder.command(json!(["seek", seconds, "relative+exact"]))?;
                        report_after = Some(Instant::now() + Duration::from_millis(500));
                    }
                    CommandKind::Volume(delta) => {
                        decoder.command(json!(["add", "volume", delta]))?;
                        decoder.command(json!(["show-progress"]))?;
                    }
                    CommandKind::Audio | CommandKind::Subtitles | CommandKind::Quality => {
                        let subtitle = matches!(command, CommandKind::Subtitles);
                        let patch = if matches!(command, CommandKind::Quality) {
                            quality = (quality + 1) % 4;
                            let value = [json!("original"), json!(1080), json!(720), json!("auto")]
                                [quality]
                                .clone();
                            options["quality"] = value.clone();
                            json!({"quality":value,"forceHls":false})
                        } else {
                            cycle_track(&descriptor, subtitle)
                        };
                        match replace(
                            &api,
                            &mut decoder,
                            &mut descriptor,
                            patch.clone(),
                            position,
                            paused,
                        ) {
                            Ok(()) => {
                                for (key, value) in patch.as_object().unwrap() {
                                    options[key] = value.clone();
                                }
                                buffering = true;
                                decoder.osd(&if matches!(command, CommandKind::Quality) {
                                    format!("Quality: {}", options["quality"])
                                } else {
                                    track_label(&descriptor, subtitle)
                                });
                            }
                            Err(error) => {
                                if !error.downcast_ref::<ApiError>().is_some_and(|e| {
                                    matches!(e.status, 400 | 422 | 429 | 503 | 504)
                                }) {
                                    return Err(error);
                                }
                                decoder.osd(&error.to_string());
                                let _ = events.send(Event::Notice(error.to_string()));
                            }
                        }
                    }
                    CommandKind::Skip => {
                        let segment = descriptor["segments"].as_array().and_then(|segments| {
                            segments.iter().find(|s| {
                                matches!(s["type"].as_str(), Some("intro" | "recap"))
                                    && number(&s["start"]).is_some_and(|start| position >= start)
                                    && number(&s["end"]).is_some_and(|end| position < end)
                            })
                        });
                        let target = segment.and_then(|s| number(&s["end"])).or_else(|| {
                            descriptor["chapters"]
                                .as_array()?
                                .iter()
                                .filter_map(|c| number(&c["start"]))
                                .find(|v| *v > position + 1.0)
                        });
                        if let Some(target) = target {
                            decoder.command(json!(["seek", target, "absolute+exact"]))?;
                        } else {
                            decoder.osd("No next chapter or intro to skip");
                        }
                    }
                }
            }
            if decoder.child.try_wait()?.is_some() {
                return Ok(false);
            }
            if let Some(event) = decoder.event()? {
                match event["event"].as_str() {
                    Some("client-message") if event["args"][0] == "tv-mode-oblecto" => {
                        if let Some(command) = match event["args"][1].as_str() {
                            Some("stop") => Some(CommandKind::Stop),
                            Some("pause") => Some(CommandKind::TogglePause),
                            Some("back") => Some(CommandKind::Seek(-10.0)),
                            Some("forward") => Some(CommandKind::Seek(10.0)),
                            Some("subtitles") => Some(CommandKind::Subtitles),
                            Some("audio") => Some(CommandKind::Audio),
                            Some("quality") => Some(CommandKind::Quality),
                            Some("skip") => Some(CommandKind::Skip),
                            _ => None,
                        } {
                            keyboard_commands.push_back(command);
                        }
                    }
                    Some("property-change") => match event["name"].as_str() {
                        Some("time-pos") => {
                            if let Some(value) = number(&event["data"]) {
                                position = value;
                            }
                        }
                        Some("pause") => {
                            paused = event["data"].as_bool().unwrap_or(paused);
                            report_after = Some(Instant::now() + Duration::from_millis(250));
                        }
                        Some("paused-for-cache") => {
                            buffering = event["data"].as_bool().unwrap_or(buffering);
                            report_after = Some(Instant::now() + Duration::from_millis(250));
                        }
                        _ => {}
                    },
                    Some("file-loaded") => {
                        buffering = true;
                        // HLS demuxers can round --start to a later segment/keyframe.
                        // Seek precisely once the new timeline is ready, including while paused.
                        decoder.command(json!([
                            "seek",
                            number(&descriptor["position"]).unwrap_or(position),
                            "absolute+exact"
                        ]))?;
                        if descriptor["method"] == "direct"
                            && let Some(subtitle) = descriptor["subtitleUrl"].as_str()
                        {
                            decoder.command(json!([
                                "sub-add",
                                api.media_url(subtitle)?,
                                "select"
                            ]))?;
                        }
                        decoder.osd("A / Space pause · ← → seek · B / Esc stop\nX subtitles · Y audio · LB quality · RB skip chapter");
                    }
                    Some("playback-restart" | "seek") => {
                        buffering = event["event"] == "seek";
                        report_after = Some(Instant::now() + Duration::from_millis(500));
                    }
                    Some("end-file") => match event["reason"].as_str() {
                        Some("eof") => {
                            position = number(&descriptor["duration"]).unwrap_or(position);
                            return Ok(true);
                        }
                        Some("error") if descriptor["method"] == "direct" && !fallback => {
                            fallback = true;
                            replace(
                                &api,
                                &mut decoder,
                                &mut descriptor,
                                json!({"forceHls":true,"quality":"auto"}),
                                position,
                                paused,
                            )?;
                        }
                        Some("error") => {
                            final_failed = true;
                            bail!("Playback failed. Try another source or a lower quality.");
                        }
                        Some("quit") => return Ok(false),
                        _ => {}
                    },
                    Some("shutdown") => return Ok(false),
                    _ => {}
                }
            }
            if last_report.elapsed() >= Duration::from_secs(10)
                || report_after.is_some_and(|deadline| Instant::now() >= deadline)
            {
                match progress(&api, &descriptor, position, paused, buffering, false) {
                    Ok(()) => {
                        last_warning.clear();
                    }
                    Err(error)
                        if !recovered
                            && error
                                .downcast_ref::<ApiError>()
                                .is_some_and(|e| e.status == 404) =>
                    {
                        recovered = true;
                        options["position"] = json!(position);
                        descriptor =
                            api.request("POST", "playback/sessions", &[], Some(options.clone()))?;
                        decoder.load(&api, &descriptor, paused)?;
                    }
                    Err(error)
                        if error
                            .downcast_ref::<ApiError>()
                            .is_some_and(|e| matches!(e.status, 401 | 409)) =>
                    {
                        return Err(error);
                    }
                    Err(error) => {
                        let warning = format!("Progress not saved: {error}");
                        if warning != last_warning {
                            decoder.osd(&warning);
                            let _ = events.send(Event::Notice(warning.clone()));
                            last_warning = warning;
                        }
                    }
                }
                last_report = Instant::now();
                report_after = None;
            }
        }
    })();
    // This ordering is required: DELETE does not persist the final position.
    let saved = progress(
        &api,
        &descriptor,
        position,
        paused,
        buffering,
        final_failed || result.is_err(),
    );
    let deleted =
        session_path(&descriptor).and_then(|path| api.request("DELETE", &path, &[], None));
    match result {
        Err(error) => Err(error),
        Ok(ended) => {
            saved.context("Playback stopped, but final progress could not be saved")?;
            deleted.context("Playback stopped, but the stream could not be released")?;
            Ok(ended)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_creation_disposes_session_without_starting_decoder() {
        let (api, server) = crate::oblecto::fixture::server(vec![
            (
                201,
                json!({"sessionId":"cancelled", "revision":1, "position":0}),
            ),
            (204, Value::Null),
            (204, Value::Null),
        ]);
        let (tx, rx) = mpsc::channel();
        tx.send(CommandKind::Stop).unwrap();
        let (events, _) = mpsc::channel();
        assert!(
            !play(
                api,
                json!({"fileId":73}),
                "/must-not-launch",
                false,
                rx,
                &events
            )
            .unwrap()
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.last().unwrap().method, "DELETE");
    }
    #[test]
    fn decoder_failure_flushes_resume_before_releasing_created_session() {
        let (api, server) = crate::oblecto::fixture::server(vec![
            (
                201,
                json!({"sessionId":"test-session", "revision":1, "position":42.5, "duration":100.0, "mediaUrl":"/media/test"}),
            ),
            (204, Value::Null),
            (204, Value::Null),
        ]);
        let (_tx, rx) = mpsc::channel();
        let (events, _) = mpsc::channel();
        assert!(
            play(
                api,
                json!({"fileId":73}),
                "/nonexistent/tv-oblecto-player",
                false,
                rx,
                &events
            )
            .is_err()
        );
        let requests = server.join().unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|r| r.method.as_str())
                .collect::<Vec<_>>(),
            vec!["POST", "POST", "DELETE"]
        );
        assert_eq!(requests[0].body["fileId"], 73);
        assert_eq!(requests[1].body["position"], 42.5);
        assert!(requests[1].path.ends_with("/progress"));
        assert!(requests[2].path.ends_with("/test-session"));
    }
    #[test]
    fn track_cycle_uses_source_indexes_and_can_disable_subtitles() {
        let mut descriptor = json!({"selectedTracks":{"subtitleStreamIndex":null},"tracks":[{"codec_type":"subtitle","index":4},{"codec_type":"subtitle","index":7}]});
        assert_eq!(
            cycle_track(&descriptor, true),
            json!({"subtitleStreamIndex":4,"subtitleMode":"auto"})
        );
        descriptor["selectedTracks"]["subtitleStreamIndex"] = json!(7);
        assert_eq!(
            cycle_track(&descriptor, true),
            json!({"subtitleStreamIndex":null,"subtitleMode":"off"})
        );
    }
}
