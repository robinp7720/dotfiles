//! One worker owns the decoder and managed session, serializing progress/PATCH/DELETE.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::oblecto::{Api, ApiError, number};

#[derive(Clone, Copy)]
pub enum CommandKind {
    TogglePause,
    Seek(f64),
    SeekTo(f64),
    AudioTrack(i64),
    SubtitleTrack(i64),
    SetQuality(u16),
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
    Status(Value),
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
pub fn start(api: Api, options: Value, socket: PathBuf) -> (Handle, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let (events, receive) = mpsc::channel();
    let join = thread::spawn(move || {
        let result = play(api, options, &socket, rx, &events);
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

// The engine lives in the Qt process. This private socket is only the control channel;
// media URLs never enter QML, a command line, or a log.
struct Decoder {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    partial: String,
}
impl Decoder {
    fn connect(socket: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket).context("The embedded player is unavailable")?;
        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            partial: String::new(),
        })
    }
    fn command(&mut self, args: Value) -> Result<()> {
        serde_json::to_writer(&mut self.writer, &json!({"command": args}))?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }
    fn event(&mut self) -> Result<Option<Value>> {
        match self.reader.read_line(&mut self.partial) {
            Ok(0) => bail!("The player closed"),
            Ok(_) => Ok(serde_json::from_str(&std::mem::take(&mut self.partial)).ok()),
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
}
impl Drop for Decoder {
    fn drop(&mut self) {
        // Leave the reusable engine and render context alive for the next title.
        let _ = self.command(json!(["stop"]));
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
fn quality_value(value: u16) -> Value {
    match value {
        0 => json!("original"),
        360 | 480 | 720 | 1080 => json!(value),
        _ => json!("auto"),
    }
}
fn status(
    descriptor: &Value,
    options: &Value,
    position: f64,
    paused: bool,
    buffering: bool,
    volume: f64,
) -> Value {
    let tracks = |kind: &str| -> Vec<Value> {
        descriptor["tracks"].as_array().into_iter().flatten()
            .filter(|t| t["codec_type"] == kind)
            .map(|t| json!({"index":t["index"], "label":t["tags"]["title"].as_str().or(t["tags"]["language"].as_str()).unwrap_or("Track")}))
            .collect()
    };
    json!({"position":position,"duration":number(&descriptor["duration"]).unwrap_or(0.0),
        "paused":paused,"buffering":buffering,"volume":volume,
        "audioLabel":track_label(descriptor, false), "subtitleLabel":track_label(descriptor, true),
        "audioTracks":tracks("audio"),"subtitleTracks":tracks("subtitle"),
        "selectedAudio":descriptor["selectedTracks"]["audioStreamIndex"],
        "selectedSubtitle":descriptor["selectedTracks"]["subtitleStreamIndex"],
        "quality":options["quality"]})
}
fn play(
    api: Api,
    mut options: Value,
    socket: &Path,
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
        let mut decoder = Decoder::connect(socket)?;
        for (index, property) in [
            (1, "time-pos"),
            (2, "pause"),
            (3, "paused-for-cache"),
            (4, "volume"),
        ] {
            decoder.command(json!(["observe_property", index, property]))?;
        }
        decoder.load(&api, &descriptor, false)?;
        let _ = events.send(Event::Ready);
        let mut last_report = Instant::now();
        let mut report_after: Option<Instant> = None;
        let mut fallback = false;
        let mut recovered = false;
        let mut last_warning = String::new();
        let mut volume = 100.0;
        let mut last_status = Instant::now() - Duration::from_secs(1);
        loop {
            for command in commands.try_iter() {
                let _ = events.send(Event::Interaction);
                match command {
                    CommandKind::Stop => {
                        position = decoder.current_position().unwrap_or(position);
                        return Ok(false);
                    }
                    CommandKind::TogglePause => {
                        paused = !paused;
                        decoder.command(json!(["set_property", "pause", paused]))?;
                    }
                    CommandKind::Seek(seconds) => {
                        decoder.command(json!(["seek", seconds, "relative+exact"]))?;
                        report_after = Some(Instant::now() + Duration::from_millis(500));
                    }
                    CommandKind::SeekTo(target) => {
                        if target.is_finite() {
                            let duration = number(&descriptor["duration"]).unwrap_or(f64::MAX);
                            decoder.command(json!([
                                "seek",
                                target.clamp(0.0, duration),
                                "absolute+exact"
                            ]))?;
                            report_after = Some(Instant::now() + Duration::from_millis(500));
                        }
                    }
                    CommandKind::Volume(delta) => {
                        decoder.command(json!(["add", "volume", delta]))?;
                    }
                    CommandKind::Audio
                    | CommandKind::Subtitles
                    | CommandKind::Quality
                    | CommandKind::AudioTrack(_)
                    | CommandKind::SubtitleTrack(_)
                    | CommandKind::SetQuality(_) => {
                        position = decoder.current_position().unwrap_or(position);
                        let patch = match command {
                            CommandKind::Quality => {
                                let choices =
                                    [json!("original"), json!(1080), json!(720), json!("auto")];
                                let next = choices
                                    .iter()
                                    .position(|q| q == &options["quality"])
                                    .map(|i| (i + 1) % choices.len())
                                    .unwrap_or(0);
                                json!({"quality":choices[next], "forceHls":true})
                            }
                            CommandKind::SetQuality(value) => {
                                json!({"quality":quality_value(value), "forceHls":true})
                            }
                            CommandKind::AudioTrack(index) => json!({"audioStreamIndex":index}),
                            CommandKind::SubtitleTrack(index) => {
                                json!({"subtitleStreamIndex":if index < 0 {Value::Null} else {json!(index)}, "subtitleMode":if index < 0 {"off"} else {"auto"}})
                            }
                            _ => {
                                cycle_track(&descriptor, matches!(command, CommandKind::Subtitles))
                            }
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
                            }
                            Err(error) => {
                                if !error.downcast_ref::<ApiError>().is_some_and(|e| {
                                    matches!(e.status, 400 | 422 | 429 | 503 | 504)
                                }) {
                                    return Err(error);
                                }
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
                            let _ = events
                                .send(Event::Notice("No next chapter or intro to skip".into()));
                        }
                    }
                }
            }
            if let Some(event) = decoder.event()? {
                match event["event"].as_str() {
                    Some("property-change") => match event["name"].as_str() {
                        Some("time-pos") => {
                            if let Some(value) = number(&event["data"]) {
                                position = value;
                            }
                        }
                        Some("volume") => volume = number(&event["data"]).unwrap_or(volume),
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
            if last_status.elapsed() >= Duration::from_millis(200) {
                let _ = events.send(Event::Status(status(
                    &descriptor,
                    &options,
                    position,
                    paused,
                    buffering,
                    volume,
                )));
                last_status = Instant::now();
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
    fn reconfiguration_flushes_old_revision_and_restores_paused_absolute_position() {
        let (api, server) = crate::oblecto::fixture::server(vec![
            (204, Value::Null),
            (
                200,
                json!({"sessionId":"test", "revision":2, "position":42.75, "method":"remux", "mediaUrl":"/media/hls.m3u8"}),
            ),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("decoder.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let engine = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut commands = Vec::new();
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                commands.push(serde_json::from_str::<Value>(&line).unwrap()["command"].clone());
            }
            commands
        });
        let mut decoder = Decoder::connect(&socket).unwrap();
        let mut descriptor = json!({"sessionId":"test", "revision":1, "duration":300});
        replace(
            &api,
            &mut decoder,
            &mut descriptor,
            json!({"quality":720,"forceHls":true}),
            42.75,
            true,
        )
        .unwrap();
        drop(decoder);
        let requests = server.join().unwrap();
        assert_eq!(requests[0].method, "POST");
        assert!(requests[0].path.ends_with("/progress"));
        assert_eq!(requests[0].body["revision"], 1);
        assert_eq!(requests[0].body["position"], 42.75);
        assert_eq!(requests[0].body["paused"], true);
        assert_eq!(requests[1].method, "PATCH");
        assert_eq!(requests[1].body["revision"], 1);
        assert_eq!(requests[1].body["position"], 42.75);
        assert_eq!(requests[1].body["forceHls"], true);
        let commands = engine.join().unwrap();
        assert_eq!(commands[0][0], "loadfile");
        assert_eq!(commands[0][4]["start"], "42.75");
        assert_eq!(commands[0][4]["pause"], "yes");
        assert_eq!(commands[1], json!(["stop"]));
    }
    #[test]
    fn presentation_state_excludes_session_urls_and_tokens() {
        let descriptor = json!({"mediaUrl":"https://example.test/?token=secret", "subtitleUrl":"secret", "sessionId":"secret", "tracks":[{"index":7,"codec_type":"audio","tags":{"language":"English"}}]});
        let value = status(
            &descriptor,
            &json!({"quality":"original"}),
            10.5,
            true,
            false,
            80.0,
        );
        assert!(!value.to_string().contains("secret"));
        assert_eq!(value["audioTracks"][0]["index"], 7);
        assert_eq!(value["position"], 10.5);
    }
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
                Path::new("/must-not-connect"),
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
                Path::new("/nonexistent/tv-oblecto-player"),
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
