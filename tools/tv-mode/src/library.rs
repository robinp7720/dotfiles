//! Asynchronous Oblecto state for the renderer. Tokens never cross the UI bridge.
use crate::{
    config::OblectoConfig,
    oblecto::{self, Api, ApiError, Kind, Media},
};
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

type Completion = Box<dyn FnOnce(&mut Library) + Send>;
type Job = (u64, Result<Completion>);
struct HistoryEntry {
    route: Value,
    browse: Option<Value>,
    cursors: HashSet<String>,
}
pub struct Library {
    pub state: Value,
    pub dirty: bool,
    pub api: Option<Api>,
    pub me: Value,
    pub selected: Option<Media>,
    pub play_request: Option<(Media, Value, f64)>,
    generation: u64,
    auth_generation: u64,
    tx: Sender<Job>,
    rx: Receiver<Job>,
    art_tx: Sender<(u64, String, Vec<u8>)>,
    art_rx: Receiver<(u64, String, Vec<u8>)>,
    cache: TempDir,
    requested: HashSet<String>,
    history: Vec<HistoryEntry>,
    cursors: HashSet<String>,
    art_order: VecDeque<String>,
    art_serial: u64,
    route: Value,
    retry_at: Option<Instant>,
}
impl Library {
    pub fn new(config: &OblectoConfig) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (art_tx, art_rx) = mpsc::channel();
        let runtime = crate::session::runtime_dir()?;
        std::fs::create_dir_all(&runtime)?;
        let cache = tempfile::Builder::new()
            .prefix("tv-art-")
            .tempdir_in(runtime)?;
        let mut this = Self {
            state: json!({"page":"home","authenticated":false,"account":"","server":config.server,"profiles":[],"rows":[],"assets":{},"loading":false,"message":"","cursor":null,"serial":0}),
            dirty: true,
            api: None,
            me: Value::Null,
            selected: None,
            play_request: None,
            generation: 0,
            auth_generation: 0,
            tx,
            rx,
            art_tx,
            art_rx,
            cache,
            requested: HashSet::new(),
            history: vec![],
            cursors: HashSet::new(),
            art_order: VecDeque::new(),
            art_serial: 0,
            route: json!({"type":"home"}),
            retry_at: None,
        };
        if let Some(profile) = oblecto::saved_profile()
            .filter(|p| Api::new(&config.server).is_ok_and(|a| a.server() == p.server))
        {
            this.job(
                move || {
                    Ok(Api::new(&profile.server)
                        .and_then(|a| a.restore(&profile.account))
                        .ok())
                },
                |s, result| {
                    if let Some((api, me)) = result {
                        s.signed_in(api, me);
                    }
                },
            );
        }
        Ok(this)
    }
    fn job<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, T) + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let generation = self.generation;
        self.state["loading"] = true.into();
        self.dirty = true;
        thread::spawn(move || {
            let result = work().map(|v| Box::new(move |s: &mut Library| done(s, v)) as Completion);
            let _ = tx.send((generation, result));
        });
    }
    pub fn poll(&mut self) {
        while let Ok((generation, result)) = self.rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.state["loading"] = false.into();
            self.dirty = true;
            match result {
                Ok(done) => done(self),
                Err(error) => self.error(error),
            }
        }
        while let Ok((generation, path, bytes)) = self.art_rx.try_recv() {
            if generation != self.auth_generation {
                continue;
            }
            // Evict individual old entries, never blank the entire library as it grows.
            while self.art_order.len() >= 128 {
                if let Some(old) = self.art_order.pop_front() {
                    self.requested.remove(&old);
                    if let Some(url) = self.state["assets"]
                        .as_object_mut()
                        .and_then(|m| m.remove(&old))
                        .and_then(|v| v.as_str().map(str::to_owned))
                        && let Ok(file) = url::Url::parse(&url).and_then(|u| {
                            u.to_file_path()
                                .map_err(|_| url::ParseError::RelativeUrlWithoutBase)
                        })
                    {
                        let _ = std::fs::remove_file(file);
                    }
                }
            }
            self.art_order.push_back(path.clone());
            if bytes.is_empty() {
                continue;
            }
            self.art_serial += 1;
            let file = self
                .cache
                .path()
                .join(format!("{}-{}.img", self.auth_generation, self.art_serial));
            if std::fs::write(&file, bytes).is_ok() {
                self.state["assets"][path] =
                    url::Url::from_file_path(file).unwrap().as_str().into();
                self.dirty = true;
            }
        }
    }
    pub fn message(&mut self, message: impl Into<String>) {
        self.state["message"] = message.into().into();
        self.dirty = true;
    }
    fn error(&mut self, error: anyhow::Error) {
        if let Some(e) = error.downcast_ref::<ApiError>() {
            if e.status == 401 && self.api.is_some() {
                self.clear_auth();
                self.route(json!({"type":"login"}), false);
            }
            if let Some(seconds) = e.retry_after {
                self.retry_at = Some(Instant::now() + Duration::from_secs(seconds.min(3600)));
            }
        }
        self.message(error.to_string());
    }
    fn clear_auth(&mut self) {
        self.generation += 1;
        self.auth_generation += 1;
        self.api = None;
        self.me = Value::Null;
        self.selected = None;
        self.history.clear();
        self.cursors.clear();
        self.art_order.clear();
        self.requested.clear();
        if let Ok(files) = std::fs::read_dir(self.cache.path()) {
            for file in files.flatten() {
                let _ = std::fs::remove_file(file.path());
            }
        }
        self.state["authenticated"] = false.into();
        self.state["account"] = "".into();
        self.state["rows"] = json!([]);
        self.state["assets"] = json!({});
        self.state["detail"] = Value::Null;
    }
    fn signed_in(&mut self, api: Api, me: Value) {
        self.clear_auth();
        self.state["server"] = api.server().into();
        self.state["authenticated"] = true.into();
        self.state["account"] = me["name"]
            .as_str()
            .or(me["username"].as_str())
            .unwrap_or("You")
            .into();
        self.api = Some(api);
        self.me = me;
        self.route(json!({"type":"home"}), false);
    }
    pub fn action(&mut self, a: Value) {
        match a["type"].as_str().unwrap_or("") {
            "login" => {
                if self.state["loading"] == true {
                    return;
                }
                if self.retry_at.is_some_and(|t| t > Instant::now()) {
                    self.message("Please wait before trying to sign in again.");
                    return;
                }
                let server = a["server"].as_str().unwrap_or("").to_string();
                let credentials = a["credentials"].clone();
                let remember = a["remember"] == true;
                self.generation += 1;
                self.message("");
                self.job(
                    move || {
                        let (api, me) = Api::new(&server)?.login(credentials)?;
                        let warning = if remember {
                            api.remember(me["username"].as_str().unwrap_or(""))
                                .err()
                                .map(|e| e.to_string())
                        } else {
                            None
                        };
                        Ok((api, me, warning))
                    },
                    |s, (api, me, warning)| {
                        s.signed_in(api, me);
                        if let Some(w) = warning {
                            s.message(w);
                        }
                    },
                );
            }
            "profiles" => {
                if let Some(server) = a["server"].as_str() {
                    self.state["server"] = server.into();
                }
                self.profiles();
            }
            "logout" => {
                if let Some(api) = self.api.clone() {
                    let account = self.me["username"].as_str().unwrap_or("").to_string();
                    self.clear_auth();
                    self.route(json!({"type":"login"}), false);
                    self.job(
                        move || Ok(api.forget(&account).err().map(|e| e.to_string())),
                        |s, e| {
                            if let Some(e) = e {
                                s.message(e);
                            }
                        },
                    );
                }
            }
            "back" => {
                if let Some(entry) = self.history.pop() {
                    if let Some(saved) = entry.browse {
                        self.restore_browse(entry.route, saved, entry.cursors);
                    } else {
                        self.route(entry.route, false);
                    }
                } else {
                    self.route(json!({"type":"home"}), false);
                }
            }
            "detail" | "browse" | "home" | "search" | "account" | "open" => self.route(a, true),
            "more" => self.more(),
            "browse_position" if self.state["page"] == "browse" => {
                self.state["browseIndex"] = a["index"].as_u64().unwrap_or(0).into();
                self.state["browseScroll"] = a["scroll"].as_f64().unwrap_or(0.0).max(0.0).into();
            }
            "artwork" => {
                if let Some(paths) = a["paths"].as_array() {
                    self.artwork(
                        paths
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .take(64)
                            .collect(),
                    );
                }
            }
            "art" => {
                if let Some(path) = a["path"].as_str() {
                    self.artwork(vec![path.to_string()]);
                }
            }
            "play" => {
                if let Some(item) = self.selected.clone() {
                    let files = item.files();
                    let index = a["source"].as_u64().unwrap_or(0) as usize;
                    if let Some(file) = files.get(index) {
                        let position = if a["restart"] == true {
                            0.0
                        } else {
                            item.resume()
                        };
                        self.play_request = Some((item, file.clone(), position));
                    }
                }
            }
            "flag" => {
                if let (Some(api), Some(item)) = (self.api.clone(), self.selected.clone()) {
                    let flag = a["flag"].as_str().unwrap_or("").to_string();
                    if !["favourite", "watchlist", "watched"].contains(&flag.as_str()) {
                        return;
                    }
                    let enabled = a["enabled"] == true;
                    let route = self.route.clone();
                    self.job(
                        move || {
                            api.request(
                                "PUT",
                                &format!("{}/{}/{flag}", item.kind.path(), item.id()),
                                &[],
                                Some(json!({flag:enabled})),
                            )
                        },
                        move |s, _| s.route(route, false),
                    );
                }
            }
            _ => {}
        }
    }
    fn profiles(&mut self) {
        self.generation += 1;
        self.state["profiles"] = json!([]);
        let server = self.state["server"].as_str().unwrap_or("").to_string();
        self.job(
            move || Api::new(&server)?.get("auth/login-options"),
            |s, options| {
                s.state["profiles"] = if options["profilePicker"] == true {
                    options["users"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into()
                } else {
                    json!([])
                };
            },
        );
    }
    fn route(&mut self, mut route: Value, push: bool) {
        let mut page = route["type"].as_str().unwrap_or("home").to_string();
        if page == "open" {
            page = if self.api.is_some() { "home" } else { "login" }.into();
            route["type"] = page.clone().into();
        }
        if self.api.is_none() && !["home", "login", "account"].contains(&page.as_str()) {
            page = "login".into();
            route = json!({"type":"login"});
        }
        if push && route != self.route {
            self.history.push(HistoryEntry {
                route: self.route.clone(),
                browse: (self.state["page"] == "browse").then(|| self.state.clone()),
                cursors: self.cursors.clone(),
            });
            let stale = self.history.len().saturating_sub(4);
            for entry in &mut self.history[..stale] {
                entry.browse = None;
            }
            if self.history.len() > 32 {
                self.history.remove(0);
            }
        }
        self.route = route.clone();
        self.generation += 1;
        self.dirty = true;
        self.state["page"] = page.clone().into();
        self.state["loading"] = false.into();
        self.state["message"] = "".into();
        self.state["cursor"] = Value::Null;
        self.state["moreLoading"] = false.into();
        self.state["moreError"] = "".into();
        self.cursors.clear();
        self.state["detail"] = Value::Null;
        self.state["serial"] = (self.state["serial"].as_u64().unwrap_or(0) + 1).into();
        if page == "login" {
            self.profiles();
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        if page == "home" {
            self.job(
                move || {
                    let mut rows = Vec::new();
                    let mut warning = None;
                    for (title, path, kind) in [
                        ("Pick up where you left off", "movies/watching", Kind::Movie),
                        ("Your next chapter", "episodes/watching", Kind::Episode),
                        ("Up next", "episodes/next", Kind::Episode),
                    ] {
                        match api.get(path).and_then(|v| oblecto::media_array(kind, v)) {
                            Ok(items) => rows.push((title.to_string(), items)),
                            Err(e) => {
                                if e.downcast_ref::<ApiError>()
                                    .is_some_and(|e| e.status == 401)
                                {
                                    return Err(e);
                                }
                                warning = Some(e.to_string());
                            }
                        }
                    }
                    for (title, kind) in [
                        ("Something worth watching", Kind::Movie),
                        ("One more episode", Kind::Series),
                    ] {
                        match api.browse(kind, "all", "", None) {
                            Ok(p) => rows.push((title.into(), p.items)),
                            Err(e) => warning = Some(e.to_string()),
                        }
                    }
                    Ok((rows, warning))
                },
                |s, (rows, warning)| {
                    s.rows(rows);
                    if let Some(w) = warning {
                        s.message(w);
                    }
                },
            );
        } else if page == "browse" {
            self.state["rows"] = json!([]);
            self.state["browseIndex"] = 0.into();
            self.state["browseScroll"] = 0.into();
            let kind = kind(&route["kind"]);
            let list = route["list"].as_str().unwrap_or("all").to_string();
            let query = route["query"].as_str().unwrap_or("").to_string();
            self.state["browseQuery"] = query.clone().into();
            let title = if !query.is_empty() {
                format!("Results for “{query}”")
            } else {
                match list.as_str() {
                    "watchlist" => "Your watchlist".into(),
                    "favourites" => "Your favourites".into(),
                    _ => if kind == Kind::Movie {
                        "Movies"
                    } else {
                        "Series"
                    }
                    .into(),
                }
            };
            self.state["browseKind"] = kind.path().into();
            self.state["browseList"] = list.clone().into();
            self.state["title"] = title.clone().into();
            self.job(
                move || api.browse(kind, &list, &query, None),
                move |s, p| {
                    s.state["cursor"] = p.cursor.into();
                    s.rows(vec![(title, p.items)]);
                },
            );
        } else if page == "detail" {
            self.state["rows"] = json!([]);
            let kind = kind(&route["kind"]);
            let id = route["id"].as_i64().unwrap_or(0);
            self.job(move||{let item=api.detail(kind,id)?;let episodes=if kind==Kind::Series{api.episodes(id)?}else{vec![]};let context=if kind==Kind::Episode{api.get(&format!("episode/{id}/context")).unwrap_or_default()}else{Value::Null};Ok((item,episodes,context))},|s,(item,episodes,context)|{
                let mut detail=s.card(&item);detail["files"]=item.files().into_iter().enumerate().map(|(i,f)|json!({"index":i,"label":if item.spoilers_hidden(&s.me["preferences"]){"Source"}else{f["name"].as_str().unwrap_or("Source")},"duration":oblecto::number(&f["duration"]).map(oblecto::timestamp).unwrap_or_default()})).collect();
                detail["resume"]=oblecto::timestamp(item.resume()).into();detail["resumable"]=(item.resume()>0.0).into();detail["favourite"]=item.data["userState"]["favourite"].clone();detail["watchlist"]=item.data["userState"]["watchlist"].clone();detail["watched"]=(item.progress()>=0.9).into();detail["next"]=context["nextUnwatched"]["id"].clone();
                s.state["detail"]=detail;s.selected=Some(item);let mut rows:Vec<(String,Vec<Media>)>=vec![];
                for e in episodes{let season=oblecto::scalar(&e.data["airedSeason"]);let title=if season=="0"{"Specials".into()}else{format!("Season {season}")};if rows.last().is_none_or(|r|r.0!=title){rows.push((title,vec![]));}rows.last_mut().unwrap().1.push(e);}
                s.rows(rows);let d=s.state["detail"].clone();s.artwork(vec![d["art"].as_str().unwrap_or("").into(),d["backdrop"].as_str().unwrap_or("").into()]);
            });
        }
    }
    fn restore_browse(&mut self, route: Value, saved: Value, cursors: HashSet<String>) {
        self.generation += 1;
        self.route = route;
        self.cursors = cursors;
        for key in [
            "rows",
            "cursor",
            "title",
            "browseKind",
            "browseList",
            "browseQuery",
            "browseIndex",
            "browseScroll",
        ] {
            self.state[key] = saved[key].clone();
        }
        self.state["page"] = "browse".into();
        self.state["loading"] = false.into();
        self.state["moreLoading"] = false.into();
        self.state["moreError"] = "".into();
        self.state["message"] = "".into();
        self.state["detail"] = Value::Null;
        self.state["serial"] = (self.state["serial"].as_u64().unwrap_or(0) + 1).into();
        self.dirty = true;
    }
    fn more(&mut self) {
        if self.state["page"] != "browse"
            || self.state["loading"] == true
            || self.state["moreLoading"] == true
        {
            return;
        }
        let (Some(api), Some(cursor)) = (
            self.api.clone(),
            self.state["cursor"].as_str().map(str::to_owned),
        ) else {
            return;
        };
        if self.cursors.contains(&cursor) {
            self.state["cursor"] = Value::Null;
            self.dirty = true;
            return;
        }
        if self.retry_at.is_some_and(|t| t > Instant::now()) {
            self.message("Please wait before retrying.");
            return;
        }
        let media_kind = kind(&self.route["kind"]);
        let list = self.route["list"].as_str().unwrap_or("all").to_owned();
        let query = self.route["query"].as_str().unwrap_or("").to_owned();
        let requested = cursor.clone();
        self.job(
            move || Ok(api.browse(media_kind, &list, &query, Some(&requested))),
            move |s, result| {
                s.state["moreLoading"] = false.into();
                match result {
                    Ok(page) => s.append_page(cursor, page),
                    Err(error) => {
                        s.state["moreError"] = error.to_string().into();
                        s.error(error);
                    }
                }
            },
        );
        self.state["loading"] = false.into();
        self.state["moreLoading"] = true.into();
        self.state["moreError"] = "".into();
    }
    fn append_page(&mut self, requested: String, page: oblecto::Page) {
        self.cursors.insert(requested);
        self.state["cursor"] = page.cursor.filter(|c| !self.cursors.contains(c)).into();
        let mut items = self.state["rows"][0]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut seen: HashSet<_> = items
            .iter()
            .map(|i| {
                (
                    i["kind"].as_str().unwrap_or("").to_owned(),
                    i["id"].as_i64().unwrap_or(0),
                )
            })
            .collect();
        for item in page.items {
            if seen.insert((item.kind.path().to_owned(), item.id())) {
                items.push(self.card(&item));
            }
        }
        self.state["rows"] = json!([{"title":self.state["title"],"items":items}]);
        self.dirty = true;
    }
    fn card(&mut self, item: &Media) -> Value {
        let hidden = item.spoilers_hidden(&self.me["preferences"]);
        let art = if hidden {
            String::new()
        } else {
            item.artwork_path()
        };
        let backdrop = if hidden {
            String::new()
        } else {
            match item.kind {
                Kind::Movie => format!("movie/{}/fanart", item.id()),
                Kind::Series => format!("series/{}/fanart", item.id()),
                Kind::Episode => art.clone(),
            }
        };
        json!({"id":item.id(),"kind":item.kind.path(),"title":item.display_title(&self.me["preferences"]),"subtitle":item.subtitle(),"overview":if hidden{"Synopsis hidden by your spoiler preferences."}else{item.data["overview"].as_str().unwrap_or("")},"art":art,"backdrop":backdrop,"progress":item.progress()})
    }
    fn rows(&mut self, rows: Vec<(String, Vec<Media>)>) {
        let mut paths = vec![];
        let result: Vec<Value> = rows
            .into_iter()
            .filter(|(_, i)| !i.is_empty())
            .map(|(title, items)| {
                let cards: Vec<Value> = items
                    .iter()
                    .map(|i| {
                        let card = self.card(i);
                        paths.push(card["art"].as_str().unwrap_or("").to_string());
                        card
                    })
                    .collect();
                json!({"title":title,"items":cards})
            })
            .collect();
        self.state["rows"] = result.into();
        self.artwork(paths);
    }
    fn artwork(&mut self, paths: Vec<String>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let generation = self.auth_generation;
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|path| {
                if path.is_empty() {
                    return false;
                }
                if self.art_order.contains(path) {
                    self.art_order.retain(|old| old != path);
                    self.art_order.push_back(path.clone());
                }
                self.requested.insert(path.clone())
            })
            .collect();
        for chunk in paths.chunks(12) {
            let api = api.clone();
            let tx = self.art_tx.clone();
            let paths = chunk.to_vec();
            thread::spawn(move || {
                for path in paths {
                    let bytes = api.artwork(&path).unwrap_or_default();
                    if tx.send((generation, path, bytes)).is_err() {
                        break;
                    }
                }
            });
        }
    }
    pub fn playback_finished(&mut self, item: &Media, ended: bool, prompt: bool) {
        self.route(
            json!({"type":"detail","kind":item.kind.path(),"id":item.id()}),
            false,
        );
        if !ended || item.kind != Kind::Episode || self.me["preferences"]["autoplayNext"] != true {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let id = item.id();
        self.job(
            move || {
                let context = api.get(&format!("episode/{id}/context"))?;
                let Some(id) = context["nextUnwatched"]["id"].as_i64() else {
                    return Ok(None);
                };
                Ok(Some(api.detail(Kind::Episode, id)?))
            },
            move |s, next| {
                if let Some(item) = next {
                    let files = item.files();
                    if !prompt && files.len() == 1 {
                        s.play_request = Some((item, files[0].clone(), 0.0));
                    } else {
                        s.message("Ready for another episode? Select Next episode to continue.");
                    }
                }
            },
        );
    }
}
fn kind(value: &Value) -> Kind {
    match value.as_str() {
        Some("series") => Kind::Series,
        Some("episode") => Kind::Episode,
        _ => Kind::Movie,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn library() -> Library {
        let (tx, rx) = mpsc::channel();
        let (art_tx, art_rx) = mpsc::channel();
        Library {
            state: json!({"page":"home","assets":{},"rows":[],"serial":0}),
            dirty: false,
            api: None,
            me: Value::Null,
            selected: None,
            play_request: None,
            generation: 0,
            auth_generation: 0,
            tx,
            rx,
            art_tx,
            art_rx,
            cache: tempfile::tempdir().unwrap(),
            requested: HashSet::new(),
            history: vec![],
            cursors: HashSet::new(),
            art_order: VecDeque::new(),
            art_serial: 0,
            route: json!({"type":"home"}),
            retry_at: None,
        }
    }
    #[test]
    fn appended_pages_keep_position_and_deduplicate_cursor_overlap() {
        let mut s = library();
        s.state = json!({"page":"browse","rows":[{"title":"Movies","items":[{"id":1,"kind":"movie","title":"One"}]}],"title":"Movies","serial":8,"browseIndex":0,"browseScroll":334});
        s.append_page(
            "page 2/+".into(),
            oblecto::Page {
                items: vec![
                    Media {
                        kind: Kind::Movie,
                        data: json!({"id":1,"movieName":"One"}),
                    },
                    Media {
                        kind: Kind::Movie,
                        data: json!({"id":2,"movieName":"Two"}),
                    },
                ],
                cursor: Some("page 3".into()),
            },
        );
        assert_eq!(s.state["rows"][0]["items"].as_array().unwrap().len(), 2);
        assert_eq!(s.state["browseScroll"], 334);
        assert_eq!(s.state["serial"], 8);
        assert!(s.history.is_empty());
        // A broken server cursor must never cause an infinite fetching loop.
        s.append_page(
            "page 3".into(),
            oblecto::Page {
                items: vec![],
                cursor: Some("page 2/+".into()),
            },
        );
        assert!(s.state["cursor"].is_null());
    }
    #[test]
    fn back_restores_loaded_catalogue_without_refetching_or_losing_focus() {
        let mut s = library();
        s.route = json!({"type":"browse","kind":"movie","list":"all"});
        s.state = json!({"page":"browse","title":"Movies","rows":[{"items":[{"id":1}]}],"cursor":"later","serial":4,"browseKind":"movie","browseList":"all","browseIndex":21,"browseScroll":668});
        s.cursors.insert("previous".into());
        s.action(json!({"type":"account"}));
        s.action(json!({"type":"back"}));
        assert_eq!(s.state["page"], "browse");
        assert_eq!(s.state["browseIndex"], 21);
        assert_eq!(s.state["browseScroll"], 668);
        assert_eq!(s.state["cursor"], "later");
        assert_eq!(s.state["rows"][0]["items"][0]["id"], 1);
        assert_eq!(s.state["loading"], false);
        assert!(s.cursors.contains("previous"));
    }
    #[test]
    fn growing_catalogue_evicts_one_old_image_without_clearing_every_cover() {
        let mut s = library();
        for i in 0..129 {
            s.requested.insert(i.to_string());
            s.art_tx.send((0, i.to_string(), vec![1])).unwrap();
        }
        s.poll();
        assert_eq!(s.state["assets"].as_object().unwrap().len(), 128);
        assert!(s.state["assets"].get("0").is_none());
        assert!(s.state["assets"].get("1").is_some());
        assert!(s.state["assets"].get("128").is_some());
        assert_eq!(std::fs::read_dir(s.cache.path()).unwrap().count(), 128);
        assert_eq!(s.auth_generation, 0);
    }

    #[test]
    fn abandoned_navigation_cannot_replace_current_screen() {
        let mut s = library();
        s.tx.send((0, Ok(Box::new(|s| s.state["page"] = "stale".into()))))
            .unwrap();
        s.action(json!({"type":"account"}));
        s.poll();
        assert_eq!(s.state["page"], "account");
    }
    #[test]
    fn sign_out_discards_late_artwork_and_never_reuses_image_urls() {
        let mut s = library();
        s.art_tx
            .send((0, "movie/1/poster".into(), vec![1]))
            .unwrap();
        s.poll();
        let first = s.state["assets"]["movie/1/poster"].clone();
        s.clear_auth();
        s.art_tx.send((0, "old-account".into(), vec![1])).unwrap();
        s.art_tx
            .send((1, "movie/1/poster".into(), vec![2]))
            .unwrap();
        s.poll();
        assert!(s.state["assets"].get("old-account").is_none());
        assert_ne!(first, s.state["assets"]["movie/1/poster"]);
        assert!(
            !url::Url::parse(first.as_str().unwrap())
                .unwrap()
                .to_file_path()
                .unwrap()
                .exists()
        );
    }
    #[test]
    fn episode_spoilers_never_reach_graphical_cards() {
        let mut s = library();
        s.me = json!({"preferences":{"episodeSpoilers":"hide-titles"}});
        let item = Media {
            kind: Kind::Episode,
            data: json!({"id":5,"episodeName":"The surprise","overview":"The ending","airedSeason":1,"airedEpisodeNumber":2,"SeriesId":8,"Files":[{"name":"secret.mkv"}]}),
        };
        let card = s.card(&item);
        assert_eq!(card["art"], "");
        assert_eq!(card["backdrop"], "");
        assert!(!card.to_string().contains("The surprise"));
        assert!(!card.to_string().contains("The ending"));
        assert!(!card.to_string().contains("secret.mkv"));
    }
}
