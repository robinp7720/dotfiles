//! Oblecto's native HTTP contract. No filesystem paths or provider IDs are playback URLs.
use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub retry_after: Option<u64>,
    message: String,
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(delay) = self.retry_after {
            write!(f, " · Try again in {delay}s")?;
        }
        Ok(())
    }
}
impl std::error::Error for ApiError {}

#[derive(Clone)]
pub struct Api {
    base: Url,
    token: Option<String>,
    agent: ureq::Agent,
}
impl Api {
    pub fn new(server: &str) -> Result<Self> {
        let server = server.trim();
        let mut base =
            Url::parse(server).context("Enter a complete http:// or https:// server address")?;
        ensure!(
            matches!(base.scheme(), "http" | "https") && base.host_str().is_some(),
            "Use an HTTP or HTTPS server address"
        );
        ensure!(
            base.username().is_empty() && base.password().is_none(),
            "Enter credentials in the sign-in fields"
        );
        base.set_query(None);
        base.set_fragment(None);
        base.set_path(&format!("{}/", base.path().trim_end_matches('/')));
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .timeout_connect(Some(Duration::from_secs(5)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            base,
            token: None,
            agent,
        })
    }
    pub fn server(&self) -> &str {
        self.base.as_str()
    }
    pub fn url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url> {
        let mut url = self.base.join(path.trim_start_matches('/'))?;
        ensure!(
            url.origin() == self.base.origin(),
            "Unexpected server origin"
        );
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        Ok(url)
    }
    pub fn media_url(&self, path: &str) -> Result<String> {
        let url = match Url::parse(path) {
            Ok(url) => url,
            Err(_) => self.url(path, &[])?,
        };
        ensure!(
            url.origin() == self.base.origin()
                && url.username().is_empty()
                && url.password().is_none(),
            "Server returned an unexpected media origin"
        );
        Ok(url.to_string())
    }
    fn bytes(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
        limit: u64,
    ) -> Result<Vec<u8>> {
        let url = self.url(path, query)?;
        let mut request = ureq::http::Request::builder()
            .method(method)
            .uri(url.as_str())
            .header("Accept", "application/json, image/*");
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let body = if let Some(body) = body {
            request = request.header("Content-Type", "application/json");
            serde_json::to_vec(&body)?
        } else {
            Vec::new()
        };
        // Do not include transport errors/URLs: media queries and login bodies can hold secrets.
        let mut response = self.agent.run(request.body(body)?).map_err(|_| {
            anyhow::anyhow!("Could not reach Oblecto. Check the server address and connection.")
        })?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());
        let bytes = response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|_| anyhow::anyhow!("Oblecto returned an incomplete or oversized response"))?;
        if !(200..300).contains(&status) {
            let error: Value = serde_json::from_slice(&bytes).unwrap_or_default();
            let message = if status == 401 && self.token.is_some() {
                "Your session expired. Sign in again.".into()
            } else {
                error["message"]
                    .as_str()
                    .or_else(|| error["error"].as_str())
                    .map(|s| s.chars().take(250).collect())
                    .unwrap_or_else(|| format!("Oblecto returned HTTP {status}"))
            };
            return Err(ApiError {
                status,
                retry_after,
                message,
            }
            .into());
        }
        Ok(bytes)
    }
    pub fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let bytes = self.bytes(method, path, query, body, 8 * 1024 * 1024)?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).context("Oblecto returned an invalid JSON response")
    }
    pub fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, &[], None)
    }
    pub fn artwork(&self, path: &str) -> Result<Vec<u8>> {
        self.bytes(
            "GET",
            path,
            &[(
                "size",
                if path.ends_with("/fanart") {
                    "large"
                } else {
                    "medium"
                },
            )],
            None,
            8 * 1024 * 1024,
        )
    }
    pub fn login(mut self, credentials: Value) -> Result<(Self, Value)> {
        let response = self.request("POST", "auth/login", &[], Some(credentials))?;
        self.token = Some(
            response["accessToken"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("Missing access token")?
                .into(),
        );
        let me = self.get("api/v1/me")?;
        Ok((self, me))
    }
    pub fn restore(mut self, account: &str) -> Result<(Self, Value)> {
        self.token = Some(secret("lookup", self.server(), account, None)?);
        let me = self.get("api/v1/me")?;
        Ok((self, me))
    }
    pub fn remember(&self, account: &str) -> Result<()> {
        secret("store", self.server(), account, self.token.as_deref())?;
        let path = profile_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            serde_json::to_vec(&SavedProfile {
                server: self.server().into(),
                account: account.into(),
            })?,
        )?;
        Ok(())
    }
    pub fn forget(&self, account: &str) -> Result<()> {
        secret("clear", self.server(), account, None)?;
        if let Ok(path) = profile_path() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
    pub fn detail(&self, kind: Kind, id: i64) -> Result<Media> {
        Ok(Media {
            kind,
            data: self.get(&format!("{}/{id}/info", kind.path()))?,
        })
    }
    pub fn browse(
        &self,
        kind: Kind,
        list: &str,
        search: &str,
        cursor: Option<&str>,
    ) -> Result<Page> {
        let path = match kind {
            Kind::Movie => "movies/list/movieName",
            Kind::Series => "series/list/seriesName",
            Kind::Episode => bail!("Episodes are browsed by series"),
        };
        let mut query = vec![
            ("mode", "browse"),
            ("order", "asc"),
            ("count", "30"),
            ("list", list),
            ("q", search),
        ];
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor));
        }
        let value = self.request("GET", path, &query, None)?;
        let items = value["items"]
            .as_array()
            .context("Missing browse results")?
            .iter()
            .cloned()
            .map(|data| Media { kind, data })
            .collect();
        Ok(Page {
            items,
            cursor: value["pageInfo"]["nextCursor"].as_str().map(String::from),
        })
    }
    pub fn episodes(&self, id: i64) -> Result<Vec<Media>> {
        let value = self.get(&format!("series/{id}/episodes"))?;
        let mut items = media_array(Kind::Episode, value)?;
        items.sort_by_key(|item| {
            (
                number(&item.data["airedSeason"])
                    .map(|v| v as i64)
                    .unwrap_or(i64::MAX),
                number(&item.data["airedEpisodeNumber"])
                    .map(|v| v as i64)
                    .unwrap_or(i64::MAX),
                item.id(),
            )
        });
        Ok(items)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Movie,
    Series,
    Episode,
}
impl Kind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
            Self::Episode => "episode",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Media {
    pub kind: Kind,
    pub data: Value,
}
impl Media {
    pub fn id(&self) -> i64 {
        self.data["id"].as_i64().unwrap_or_default()
    }
    pub fn title(&self) -> &str {
        let key = match self.kind {
            Kind::Movie => "movieName",
            Kind::Series => "seriesName",
            Kind::Episode => "episodeName",
        };
        self.data[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("Untitled")
    }
    pub fn progress(&self) -> f64 {
        number(&self.track()["progress"])
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
    pub fn track(&self) -> &Value {
        &self.data[if self.kind == Kind::Movie {
            "TrackMovies"
        } else {
            "TrackEpisodes"
        }][0]
    }
    pub fn resume(&self) -> f64 {
        if self.progress() > 0.0 && self.progress() < 0.9 {
            number(&self.track()["time"]).unwrap_or(0.0).max(0.0)
        } else {
            0.0
        }
    }
    pub fn spoilers_hidden(&self, preferences: &Value) -> bool {
        self.kind == Kind::Episode
            && self.progress() < 0.9
            && matches!(
                preferences["episodeSpoilers"].as_str(),
                Some("hide" | "hide-titles")
            )
    }
    pub fn display_title(&self, preferences: &Value) -> String {
        if self.spoilers_hidden(preferences) && preferences["episodeSpoilers"] == "hide-titles" {
            self.episode_label()
        } else {
            self.title().into()
        }
    }
    pub fn episode_label(&self) -> String {
        format!(
            "S{} · E{}",
            scalar(&self.data["airedSeason"]),
            scalar(&self.data["airedEpisodeNumber"])
        )
    }
    pub fn subtitle(&self) -> String {
        if self.kind == Kind::Episode {
            return format!(
                "{} · {}",
                self.data["Series"]["seriesName"]
                    .as_str()
                    .unwrap_or("Episode"),
                self.episode_label()
            );
        }
        let date = self.data[if self.kind == Kind::Movie {
            "releaseDate"
        } else {
            "firstAired"
        }]
        .as_str()
        .unwrap_or("");
        let mut bits = vec![date.chars().take(4).collect::<String>()];
        if let Some(runtime) = number(&self.data["runtime"]) {
            bits.push(format!("{runtime:.0} min"));
        }
        if let Some(rating) = number(&self.data["siteRating"]) {
            bits.push(format!("★ {rating:.1}"));
        }
        bits.into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
    pub fn artwork_path(&self) -> String {
        if self.kind == Kind::Episode {
            if let Some(id) = self.data["SeriesId"]
                .as_i64()
                .or_else(|| self.data["Series"]["id"].as_i64())
            {
                return format!("series/{id}/poster");
            }
            return format!("episode/{}/banner", self.id());
        }
        format!("{}/{}/poster", self.kind.path(), self.id())
    }
    pub fn files(&self) -> Vec<Value> {
        self.data["Files"].as_array().cloned().unwrap_or_default()
    }
}
#[derive(Clone)]
pub struct Page {
    pub items: Vec<Media>,
    pub cursor: Option<String>,
}
pub fn media_array(kind: Kind, value: Value) -> Result<Vec<Media>> {
    Ok(value
        .as_array()
        .context("Missing library results")?
        .iter()
        .cloned()
        .map(|data| Media { kind, data })
        .collect())
}
pub fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite())
}
pub fn scalar(value: &Value) -> String {
    value
        .as_str()
        .map(String::from)
        .or_else(|| value.as_i64().map(|v| v.to_string()))
        .unwrap_or_else(|| "?".into())
}
pub fn timestamp(seconds: f64) -> String {
    if seconds > 0.0 && seconds < 60.0 {
        return format!("{seconds:.0}s");
    }
    let minutes = (seconds.max(0.0) / 60.0) as u64;
    if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}
pub fn playback_options(file: &Value, preferences: &Value, position: f64) -> Value {
    let mut body = json!({"fileId": file["id"], "position": position,
        "quality": preferences.get("quality").unwrap_or(&json!("original")),
        "subtitleMode": preferences.get("subtitleMode").unwrap_or(&json!("auto")),
        "capabilities": {"containers": ["mp4", "m4v", "mkv", "webm", "avi", "mov"],
            "videoCodecs": ["h264", "hevc", "vp8", "vp9", "av1", "mpeg2video", "mpeg4"],
            "audioCodecs": ["aac", "ac3", "eac3", "dts", "flac", "mp3", "opus", "vorbis", "pcm_s16le"],
            "maxHeight": 2160, "maxBitDepth": 10, "maxAudioChannels": 2,
            "hdr": false, "nativeTracks": false, "hls": true}});
    for (preference, kind, key) in [
        ("audioLanguage", "audio", "audioStreamIndex"),
        ("subtitleLanguage", "subtitle", "subtitleStreamIndex"),
    ] {
        if let Some(language) = preferences[preference].as_str()
            && let Some(streams) = file["Streams"].as_array()
            && let Some(stream) = streams.iter().find(|s| {
                s["codec_type"] == kind
                    && s["tags_language"]
                        .as_str()
                        .is_some_and(|v| v.eq_ignore_ascii_case(language))
            })
            && let Some(index) = stream["index"].as_u64()
        {
            body[key] = json!(index);
        }
    }
    body
}

#[derive(Serialize, Deserialize)]
pub struct SavedProfile {
    pub server: String,
    pub account: String,
}
fn profile_path() -> Result<PathBuf> {
    Ok(dirs::state_dir()
        .context("No user state directory")?
        .join("tv-mode/oblecto-profile.json"))
}
pub fn saved_profile() -> Option<SavedProfile> {
    serde_json::from_slice(&std::fs::read(profile_path().ok()?).ok()?).ok()
}
fn secret(action: &str, server: &str, account: &str, token: Option<&str>) -> Result<String> {
    let mut command = Command::new("secret-tool");
    command.arg(action);
    if action == "store" {
        command.arg("--label=TV mode · Oblecto");
    }
    let mut child = command
        .args([
            "application",
            "tv-mode-oblecto",
            "server",
            server,
            "account",
            account,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("The desktop keyring is unavailable (secret-tool is required)")?;
    if let Some(mut stdin) = child.stdin.take()
        && let Some(token) = token
    {
        stdin.write_all(token.as_bytes())?;
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("The desktop keyring did not unlock. Sign in for this session instead.");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "No saved sign-in is available in the desktop keyring"
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_preserve_proxy_prefix_and_reject_foreign_media() {
        let api = Api::new("https://example.org/media?ignored=1").unwrap();
        assert_eq!(
            api.url("/movies/list/movieName", &[("q", "a & b")])
                .unwrap()
                .as_str(),
            "https://example.org/media/movies/list/movieName?q=a+%26+b"
        );
        assert_eq!(
            api.media_url("/playback/file?token=abc").unwrap(),
            "https://example.org/media/playback/file?token=abc"
        );
        assert!(api.media_url("https://evil.example/file").is_err());
        assert!(Api::new("file:///etc/passwd").is_err());
        assert!(Api::new("https://user:password@example.org").is_err());
    }
    #[test]
    fn resume_uses_fractional_track_and_hides_unwatched_spoilers() {
        let mut media = Media {
            kind: Kind::Episode,
            data: json!({"episodeName":"Secret", "TrackEpisodes":[{"time":125.5,"progress":0.2}],"airedSeason":"2","airedEpisodeNumber":"10"}),
        };
        assert_eq!(media.resume(), 125.5);
        assert_eq!(
            media.display_title(&json!({"episodeSpoilers":"hide-titles"})),
            "S2 · E10"
        );
        media.data["TrackEpisodes"][0]["progress"] = json!(0.9);
        assert_eq!(media.resume(), 0.0);
        assert!(!media.spoilers_hidden(&json!({"episodeSpoilers":"hide"})));
    }
    #[test]
    fn playback_uses_file_id_and_absolute_stream_indexes() {
        let options = playback_options(
            &json!({"id":73,"Streams":[{"id":900,"index":3,"codec_type":"audio","tags_language":"deu"}]}),
            &json!({"quality":720,"audioLanguage":"deu"}),
            121.2,
        );
        assert_eq!(options["fileId"], 73);
        assert_eq!(options["audioStreamIndex"], 3);
        assert_eq!(options["position"], 121.2);
        assert_eq!(options["quality"], 720);
        assert_eq!(options["capabilities"]["nativeTracks"], false);
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;
    pub struct Request {
        pub method: String,
        pub path: String,
        pub authorization: Option<String>,
        pub body: Value,
    }
    pub fn server(responses: Vec<(u16, Value)>) -> (Api, thread::JoinHandle<Vec<Request>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = Api::new(&format!("http://{}/prefix", listener.local_addr().unwrap())).unwrap();
        let join = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, value) in responses {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("fixture request missing: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                let mut authorization = None;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                        if key.eq_ignore_ascii_case("authorization") {
                            authorization = Some(value.trim().into());
                        }
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let fields = first.split_whitespace().collect::<Vec<_>>();
                requests.push(Request {
                    method: fields[0].into(),
                    path: fields[1].into(),
                    authorization,
                    body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                });
                let bytes = if status == 204 {
                    vec![]
                } else {
                    serde_json::to_vec(&value).unwrap()
                };
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nRetry-After: 2\r\n\r\n",bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
            }
            requests
        });
        (api, join)
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    #[test]
    fn auth_browse_cursor_and_empty_progress_use_native_contract() {
        let (api, server) = fixture::server(vec![
            (200, json!({"accessToken":"test-token"})),
            (200, json!({"username":"viewer"})),
            (
                200,
                json!({"items":[{"id":42,"movieName":null}],"pageInfo":{"nextCursor":"next"}}),
            ),
            (204, Value::Null),
        ]);
        let (api, me) = api
            .login(json!({"username":"viewer","password":"test-only"}))
            .unwrap();
        assert_eq!(me["username"], "viewer");
        let page = api
            .browse(Kind::Movie, "watchlist", "a & b", Some("opaque/+ cursor"))
            .unwrap();
        assert_eq!(page.items[0].title(), "Untitled");
        assert_eq!(page.cursor.as_deref(), Some("next"));
        assert_eq!(
            api.request(
                "POST",
                "playback/sessions/test/progress",
                &[],
                Some(json!({"revision":1,"position":12.5}))
            )
            .unwrap(),
            Value::Null
        );
        let requests = server.join().unwrap();
        assert_eq!(requests[0].path, "/prefix/auth/login");
        assert!(requests[0].authorization.is_none());
        assert_eq!(
            requests[1].authorization.as_deref(),
            Some("Bearer test-token")
        );
        assert!(requests[2].path.contains("cursor=opaque%2F%2B+cursor"));
        assert!(requests[2].path.contains("q=a+%26+b"));
        assert_eq!(requests[3].body["position"], 12.5);
    }
    #[test]
    fn errors_keep_status_and_retry_delay_without_automatic_retries() {
        let (api, server) = fixture::server(vec![(429, json!({"error":"Please wait"}))]);
        let error = api.login(json!({"username":"viewer"})).err().unwrap();
        let error = error.downcast_ref::<ApiError>().unwrap();
        assert_eq!(error.status, 429);
        assert_eq!(error.retry_after, Some(2));
        assert_eq!(server.join().unwrap().len(), 1);
    }
    #[test]
    fn episode_order_is_numeric_with_specials_and_unknowns() {
        let (api, server) = fixture::server(vec![(
            200,
            json!([
                {"id":10,"airedSeason":"1","airedEpisodeNumber":"10"},
                {"id":2,"airedSeason":"1","airedEpisodeNumber":"2"},
                {"id":3,"airedSeason":"10","airedEpisodeNumber":"1"},
                {"id":4,"airedSeason":"2","airedEpisodeNumber":"1"},
                {"id":5,"airedSeason":"0","airedEpisodeNumber":"1"},
                {"id":6,"airedSeason":null,"airedEpisodeNumber":null}
            ]),
        )]);
        assert_eq!(
            api.episodes(1)
                .unwrap()
                .iter()
                .map(Media::id)
                .collect::<Vec<_>>(),
            vec![5, 2, 10, 4, 3, 6]
        );
        server.join().unwrap();
    }
}
