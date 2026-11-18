//! The NewsQueue HTTP API (`Api`) and web UI files, shared by the
//! `newsqueue-server` binary and the desktop app's "Web access" setting.
//! See the binary for the endpoint list.

use newsqueue_core::command::{Command, MoveDir};
#[cfg(test)]
use newsqueue_core::engine::Engine;
#[cfg(test)]
use newsqueue_core::host::EngineHost;
use newsqueue_core::model::{app_base, parse_nzb, Settings, VERSION};
use newsqueue_core::snapshot::EngineHandle;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_BIND: &str = "127.0.0.1:8480";
/// Largest NZB accepted by `POST /api/nzb`.
pub const MAX_NZB_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Api {
    pub handle: EngineHandle,
    pub api_key: Option<String>,
    /// The egui web build (`index.html` + `pkg/`), if there is one.
    pub web_dir: Option<PathBuf>,
}

/// `--web DIR`, else `web` beside the executable, else the dev build in `app/web/dist`.
pub fn web_dir(arg: Option<String>) -> Option<PathBuf> {
    let candidates = [
        arg.map(PathBuf::from),
        std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("web"))),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("web").join("dist")),
    ];
    candidates.into_iter().flatten().find(|d| d.join("index.html").is_file())
}

/// A file of the web build. Only plain names under the folder; no `..`.
fn static_file(dir: &std::path::Path, path: &str) -> Option<(Vec<u8>, &'static str)> {
    let rel = path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if rel.split('/').any(|p| p.is_empty() || p == ".." || p.contains(['\\', ':']) || p.contains(':')) {
        return None;
    }
    let ty = match rel.rsplit('.').next()? {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => return None,
    };
    std::fs::read(dir.join(rel)).ok().map(|b| (b, ty))
}

pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
}

/// The web UI: one self-contained page that talks to the JSON API.
pub const INDEX_HTML: &str = include_str!("index.html");

fn json(status: u16, value: serde_json::Value) -> Reply {
    Reply { status, body: value.to_string().into_bytes(), content_type: "application/json" }
}

fn error(status: u16, message: &str) -> Reply {
    json(status, serde_json::json!({ "error": message }))
}

fn accepted() -> Reply {
    json(202, serde_json::json!({ "queued": true }))
}

/// Compares without stopping at the first different byte.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Decodes `%XX` and `+` in a query value.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = |c: u8| (c as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| percent_decode(v))
}

/// The `nq_key` value from a Cookie header.
fn cookie_key(cookie: &str) -> Option<String> {
    cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == "nq_key")
        .map(|(_, value)| percent_decode(value))
}

/// `GET /api/events`: a snapshot as `data:` each time it changes (checked
/// four times a second), and a comment every 15 s so proxies keep it open.
struct EventStream {
    handle: EngineHandle,
    last: String,
    buf: Vec<u8>,
    quiet_since: std::time::Instant,
}

impl EventStream {
    fn new(handle: EngineHandle) -> Self {
        Self { handle, last: String::new(), buf: b"retry: 2000\n\n".to_vec(), quiet_since: std::time::Instant::now() }
    }

    /// The next message, or None when nothing changed yet.
    fn next_message(&mut self) -> Option<Vec<u8>> {
        let mut snapshot = self.handle.snapshot();
        snapshot.seq = 0;
        let text = serde_json::to_string(&snapshot).unwrap_or_default();
        if text != self.last {
            self.last = text;
            self.quiet_since = std::time::Instant::now();
            return Some(format!("data: {}\n\n", self.last).into_bytes());
        }
        if self.quiet_since.elapsed() >= Duration::from_secs(15) {
            self.quiet_since = std::time::Instant::now();
            return Some(b": keep-alive\n\n".to_vec());
        }
        None
    }
}

impl EventStream {
    /// Writes the stream until the client goes away. tiny_http's chunked
    /// responses buffer 8 KB, so this writes a plain `Connection: close`
    /// response itself and flushes every message.
    fn run(mut self, mut out: Box<dyn std::io::Write + Send>) {
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
        if out.write_all(head.as_bytes()).is_err() {
            return;
        }
        let mut pending = std::mem::take(&mut self.buf);
        loop {
            if !pending.is_empty() {
                if out.write_all(&pending).and_then(|_| out.flush()).is_err() {
                    return;
                }
                pending.clear();
            }
            match self.next_message() {
                Some(message) => pending = message,
                None => std::thread::sleep(Duration::from_millis(250)),
            }
        }
    }
}

impl Api {
    fn authorized(&self, query: &str, header_key: Option<&str>) -> bool {
        let Some(expected) = &self.api_key else {
            return true;
        };
        let given = header_key.map(str::to_string).or_else(|| query_value(query, "apikey")).unwrap_or_default();
        same_secret(&given, expected)
    }

    fn serve(&self, mut request: tiny_http::Request) {
        let method = request.method().as_str().to_ascii_uppercase();
        let url = request.url().to_string();
        let header = |name: &'static str| {
            request
                .headers()
                .iter()
                .find(|h| h.field.equiv(name))
                .map(|h| h.value.as_str().to_string())
        };
        let header_key = header("X-Api-Key").or_else(|| header("Cookie").and_then(|c| cookie_key(&c)));
        let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
        if method == "GET" && path == "/api/events" {
            if !self.authorized(query, header_key.as_deref()) {
                let _ = request.respond(tiny_http::Response::from_string("missing or wrong API key").with_status_code(401));
                return;
            }
            EventStream::new(self.handle.clone()).run(request.into_writer());
            return;
        }
        if method == "POST" && path == "/api/login" {
            let mut given = String::new();
            let _ = request.as_reader().take(1024).read_to_string(&mut given);
            let given = given.trim().to_string();
            let ok = match &self.api_key {
                Some(expected) => same_secret(&given, expected),
                None => true,
            };
            let reply = if ok { json(200, serde_json::json!({ "ok": true })) } else { error(401, "wrong API key") };
            let mut response = tiny_http::Response::from_data(reply.body)
                .with_status_code(reply.status)
                .with_header(tiny_http::Header::from_bytes("Content-Type", reply.content_type).expect("header"));
            if ok {
                let cookie = format!("nq_key={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000", url_encode(&given));
                if let Ok(h) = tiny_http::Header::from_bytes("Set-Cookie", cookie) {
                    response = response.with_header(h);
                }
            }
            let _ = request.respond(response);
            return;
        }
        let mut body = Vec::new();
        let too_big = request
            .as_reader()
            .take(MAX_NZB_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map(|n| n > MAX_NZB_BYTES)
            .unwrap_or(true);
        let reply = if too_big {
            error(413, "request body too large or unreadable")
        } else {
            self.route(&method, &url, header_key.as_deref(), &body)
        };
        let content_type =
            tiny_http::Header::from_bytes("Content-Type", reply.content_type).expect("header");
        let response = tiny_http::Response::from_data(reply.body)
            .with_status_code(reply.status)
            .with_header(content_type);
        let _ = request.respond(response);
    }

    /// Routes one request. Pure apart from sending commands, so it is tested
    /// without a socket.
    pub fn route(&self, method: &str, url: &str, header_key: Option<&str>, body: &[u8]) -> Reply {
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        // The page holds no data; it asks for the API key and sends it itself.
        if method == "GET" && !path.starts_with("/api/") {
            if path == "/lite" {
                return Reply { status: 200, body: INDEX_HTML.as_bytes().to_vec(), content_type: "text/html; charset=utf-8" };
            }
            if let Some((body, content_type)) = self.web_dir.as_deref().and_then(|d| static_file(d, path)) {
                return Reply { status: 200, body, content_type };
            }
            if path == "/" || path == "/index.html" {
                return Reply { status: 200, body: INDEX_HTML.as_bytes().to_vec(), content_type: "text/html; charset=utf-8" };
            }
        }
        if path == "/api/health" {
            return json(200, serde_json::json!({ "ok": true, "version": VERSION }));
        }
        if !self.authorized(query, header_key) {
            return error(401, "missing or wrong API key");
        }
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        let send = |command: Command| {
            if self.handle.send(command) {
                accepted()
            } else {
                error(503, "engine stopped")
            }
        };
        match (method, parts.as_slice()) {
            ("GET", ["api", "state"]) => match serde_json::to_value(self.handle.snapshot()) {
                Ok(v) => json(200, v),
                Err(e) => error(500, &e.to_string()),
            },
            ("POST", ["api", "pause"]) | ("POST", ["api", "resume"]) => {
                let want_paused = parts[1] == "pause";
                if self.handle.read(|s| s.paused) == want_paused {
                    json(200, serde_json::json!({ "paused": want_paused }))
                } else {
                    send(Command::TogglePause)
                }
            }
            ("POST", ["api", "connect"]) => send(Command::Connect),
            ("POST", ["api", "disconnect"]) => send(Command::Disconnect),
            ("POST", ["api", "cleanup"]) => send(Command::Cleanup),
            ("POST", ["api", "pause-all"]) => send(Command::PauseAll(true)),
            ("POST", ["api", "resume-all"]) => send(Command::PauseAll(false)),
            ("POST", ["api", "pause-for"]) => match query_value(query, "minutes").and_then(|m| m.parse::<u32>().ok()) {
                Some(m) if m > 0 => send(Command::PauseFor(m)),
                _ => error(400, "pause-for needs ?minutes=<1..>"),
            },
            ("POST", ["api", "rss", "timer"]) => send(Command::SetRssTimer(query_value(query, "on").as_deref() != Some("0"))),
            ("POST", ["api", "par2", index, "start"]) => match index.parse::<usize>() {
                Ok(i) if self.handle.read(|s| i < s.par2.len()) => send(Command::StartPar2Set(i)),
                Ok(_) => error(404, "no such PAR2 set"),
                Err(_) => error(400, "bad PAR2 index"),
            },
            ("POST", ["api", "speed-limit"]) => match query_value(query, "tag").as_deref() {
                None | Some("") | Some("none") => send(Command::SetSpeedLimit(None)),
                Some(tag) => match tag.parse::<u8>() {
                    Ok(t) => send(Command::SetSpeedLimit(Some(t))),
                    Err(_) => error(400, "tag must be a number or none"),
                },
            },
            ("POST", ["api", "nzb"]) => self.add_nzb(query, body),
            ("POST", ["api", "nzb", "url"]) => self.add_nzb_url(query, body),
            ("GET", ["api", "history"]) => json(200, self.handle.read(|s| serde_json::to_value(&s.history).unwrap_or_default())),
            ("GET", ["api", "log"]) => json(200, self.handle.read(|s| serde_json::to_value(&s.log_tail).unwrap_or_default())),
            ("GET", ["api", "settings"]) => json(200, self.handle.read(|s| s.settings.clone())),
            ("POST" | "PUT", ["api", "settings"]) => {
                let Ok(patch) = serde_json::from_slice::<serde_json::Value>(body) else {
                    return error(400, "send a JSON object");
                };
                // Check names and types against the current public settings.
                let current: Settings = self
                    .handle
                    .read(|s| serde_json::from_value(s.settings.clone()))
                    .unwrap_or_default();
                if let Err(e) = current.merged(&patch) {
                    return error(400, &e);
                }
                send(Command::MergeSettings(patch))
            }
            ("GET", ["api", "servers"]) => json(
                200,
                self.handle.read(|s| {
                    serde_json::Value::Array(
                        s.servers
                            .iter()
                            .map(|r| {
                                let mut v = serde_json::to_value(r).unwrap_or_default();
                                v["username"] = r.username.clone().into();
                                v
                            })
                            .collect(),
                    )
                }),
            ),
            ("POST", ["api", "servers"]) => match serde_json::from_slice::<Vec<serde_json::Value>>(body) {
                Ok(list) if list.iter().all(|v| v.is_object()) => send(Command::SetServers(list)),
                _ => error(400, "send a JSON array of server objects"),
            },
            ("GET", ["api", "rss"]) => json(200, self.handle.read(|s| serde_json::json!(s.rss_urls))),
            ("POST", ["api", "rss"]) => match serde_json::from_slice::<Vec<String>>(body) {
                Ok(urls) => send(Command::SetRssUrls(urls)),
                Err(_) => error(400, "send a JSON array of feed URLs"),
            },
            ("POST", ["api", "rss", "refresh"]) => send(Command::RefreshRss),
            ("POST", ["api", "expand"]) => send(Command::SetAllExpanded(query_value(query, "on").as_deref() != Some("0"))),
            ("POST", ["api", "history", index, "retry"]) => {
                let Ok(index) = index.parse::<usize>() else {
                    return error(400, "bad history index");
                };
                if index >= self.handle.read(|s| s.history.len()) {
                    return error(404, "no such history entry");
                }
                send(Command::RetryHistory(index))
            }
            ("POST", ["api", "queue", id, action]) => {
                let Ok(id) = id.parse::<u64>() else {
                    return error(400, "bad collection id");
                };
                if !self.handle.read(|s| s.queue.iter().any(|r| r.id == id)) {
                    return error(404, "no such collection");
                }
                let command = match *action {
                    "pause" => Command::Pause(id, true),
                    "resume" => Command::Pause(id, false),
                    "delete" => Command::Delete(id),
                    "up" => Command::Move(id, MoveDir::Up),
                    "down" => Command::Move(id, MoveDir::Down),
                    "top" => Command::Move(id, MoveDir::Top),
                    "bottom" => Command::Move(id, MoveDir::Bottom),
                    "unpack" => Command::Unpack(id),
                    "expand" => Command::SetExpanded(id, true),
                    "collapse" => Command::SetExpanded(id, false),
                    "move" => match query_value(query, "to").and_then(|t| t.parse::<usize>().ok()) {
                        Some(to) => Command::Move(id, MoveDir::To(to)),
                        None => return error(400, "move needs ?to=<row>"),
                    },
                    "rename" => {
                        let name = String::from_utf8_lossy(body).trim().to_string();
                        if name.is_empty() {
                            return error(400, "send the new name as the request body");
                        }
                        Command::Rename(id, name)
                    }
                    "password" => {
                        let password = String::from_utf8_lossy(body).trim().to_string();
                        if password.is_empty() {
                            return error(400, "send the password as the request body");
                        }
                        Command::SubmitPassword(id, password)
                    }
                    _ => return error(404, "unknown queue action"),
                };
                send(command)
            }
            _ => error(404, "not found"),
        }
    }

    /// `POST /api/nzb/url?url=https://...` (or the URL as the body): the
    /// server downloads the NZB and queues it like an upload.
    fn add_nzb_url(&self, query: &str, body: &[u8]) -> Reply {
        let url = query_value(query, "url").unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return error(400, "send an http(s) URL as ?url= or the body");
        }
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(60)).build();
        let response = match agent.get(&url).call() {
            Ok(r) => r,
            // The URL may hold an indexer key: never echo it.
            Err(ureq::Error::Status(code, _)) => return error(502, &format!("the NZB server answered {code}")),
            Err(_) => return error(502, "cannot fetch the NZB"),
        };
        let disposition = response.header("Content-Disposition").unwrap_or("").to_string();
        let mut data = Vec::new();
        if response
            .into_reader()
            .take(MAX_NZB_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map(|n| n > MAX_NZB_BYTES)
            .unwrap_or(true)
        {
            return error(413, "NZB too large or unreadable");
        }
        let name = query_value(query, "name")
            .or_else(|| disposition.split("filename=").nth(1).map(|n| n.trim_matches(['"', ';', ' ']).to_string()))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                let path = url.split(['?', '#']).next().unwrap_or("");
                path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("download.nzb").to_string()
            });
        let paused = matches!(query_value(query, "paused").as_deref(), Some("1") | Some("true"));
        self.add_nzb_named(&name, paused, &data)
    }

    /// `POST /api/nzb?name=x.nzb&paused=1` with the NZB XML as the body.
    fn add_nzb(&self, query: &str, body: &[u8]) -> Reply {
        let name = query_value(query, "name").unwrap_or_else(|| "upload.nzb".into());
        let paused = matches!(query_value(query, "paused").as_deref(), Some("1") | Some("true"));
        self.add_nzb_named(&name, paused, body)
    }

    fn add_nzb_named(&self, name: &str, paused: bool, body: &[u8]) -> Reply {
        if body.is_empty() {
            return error(400, "send the NZB file as the request body");
        }
        let stem: String = name
            .trim_end_matches(".nzb")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' })
            .take(120)
            .collect();
        let stem = if stem.trim_matches(['.', '_']).is_empty() { "upload".into() } else { stem };
        let dir: PathBuf = app_base().join("nzb").join("uploads");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return error(500, &format!("cannot create {}: {e}", dir.display()));
        }
        let mut path = dir.join(format!("{stem}.nzb"));
        let mut n = 1;
        while path.exists() {
            path = dir.join(format!("{stem}.{n}.nzb"));
            n += 1;
        }
        if let Err(e) = std::fs::write(&path, body) {
            return error(500, &format!("cannot save the NZB: {e}"));
        }
        // Reject a broken file now rather than "queueing" it.
        match parse_nzb(&path) {
            Ok(nzb) if !nzb.files.is_empty() => {}
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
                return error(400, "the NZB lists no files");
            }
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return error(400, &format!("not a valid NZB: {e}"));
            }
        }
        let command = Command::AddNzbFile(path.to_string_lossy().to_string(), paused);
        if self.handle.send(command) {
            json(202, serde_json::json!({ "queued": true, "file": path.file_name().map(|f| f.to_string_lossy().to_string()) }))
        } else {
            error(503, "engine stopped")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> (EngineHost, Api) {
        let host = EngineHost::new(Engine::new(Settings::default(), true));
        let api = Api { handle: host.handle(), api_key: Some("k3y".into()), web_dir: None };
        (host, api)
    }

    #[test]
    fn health_is_open_and_the_rest_needs_the_key() {
        let (_host, api) = demo();
        assert_eq!(api.route("GET", "/api/health", None, b"").status, 200);
        let page = api.route("GET", "/", None, b"");
        assert_eq!(page.status, 200);
        assert!(page.content_type.starts_with("text/html"));
        assert_eq!(api.route("GET", "/lite", None, b"").status, 200);
        assert_eq!(api.route("GET", "/api/state", None, b"").status, 401);
        assert_eq!(api.route("GET", "/api/state", Some("nope"), b"").status, 401);
        assert_eq!(api.route("GET", "/api/state", Some("k3y"), b"").status, 200);
        assert_eq!(api.route("GET", "/api/state?apikey=k3y", None, b"").status, 200);
    }

    #[test]
    fn static_files_stay_inside_the_web_folder() {
        let dir = std::env::temp_dir().join(format!("nq-web-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>egui</html>").unwrap();
        std::fs::write(dir.join("pkg").join("a.wasm"), [0, 97, 115, 109]).unwrap();
        assert_eq!(static_file(&dir, "/").unwrap().1, "text/html; charset=utf-8");
        assert_eq!(static_file(&dir, "/pkg/a.wasm").unwrap().1, "application/wasm");
        assert!(static_file(&dir, "/../secret.html").is_none());
        assert!(static_file(&dir, "/pkg//a.wasm").is_none());
        assert!(static_file(&dir, "/newsqueue-settings.json").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_has_the_queue_and_no_secrets() {
        let (_host, api) = demo();
        let r = api.route("GET", "/api/state", Some("k3y"), b"");
        let v: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        assert!(v["queue"].as_array().unwrap().len() >= 3);
        assert!(!String::from_utf8_lossy(&r.body).contains("\"password\""));
    }

    #[test]
    fn queue_actions_become_engine_commands() {
        let (mut host, api) = demo();
        let ids: Vec<u64> = host.queue.iter().map(|c| c.id).collect();
        let last = *ids.last().unwrap();
        assert_eq!(api.route("POST", &format!("/api/queue/{last}/top"), Some("k3y"), b"").status, 202);
        assert_eq!(api.route("POST", &format!("/api/queue/{}/delete", ids[0]), Some("k3y"), b"").status, 202);
        assert_eq!(api.route("POST", "/api/queue/999999/delete", Some("k3y"), b"").status, 404);
        assert_eq!(api.route("POST", &format!("/api/queue/{last}/explode"), Some("k3y"), b"").status, 404);
        assert_eq!(api.route("POST", &format!("/api/queue/{last}/password"), Some("k3y"), b"  ").status, 400);
        host.tick_local();
        assert_eq!(host.queue[0].id, last);
        assert!(host.queue.iter().all(|c| c.id != ids[0]));
        let snap = api.handle.snapshot();
        assert_eq!(snap.queue[0].id, last);
    }

    #[test]
    fn servers_and_feeds_keep_passwords_private() {
        let (mut host, api) = demo();
        host.settings.servers = vec![newsqueue_core::model::Server {
            name: "main".into(),
            address: "news.example".into(),
            username: "u1".into(),
            password: "s3cret".into(),
            nic: "eth9".into(),
            ..Default::default()
        }];
        host.settings.rss_urls = vec!["https://feed.example/rss?key=abc".into()];
        host.tick_local();
        let state = api.route("GET", "/api/state", Some("k3y"), b"");
        let text = String::from_utf8_lossy(&state.body).to_string();
        assert!(!text.contains("s3cret") && !text.contains("u1") && !text.contains("key=abc"));
        let servers = api.route("GET", "/api/servers", Some("k3y"), b"");
        let v: serde_json::Value = serde_json::from_slice(&servers.body).unwrap();
        assert_eq!(v[0]["username"], "u1");
        assert_eq!(v[0]["password_set"], true);
        assert!(!String::from_utf8_lossy(&servers.body).contains("s3cret"));
        // Rename without a password: the password and unknown fields stay.
        let body = br#"[{"index":0,"name":"renamed","password":""},{"name":"second","address":"b.example"}]"#;
        assert_eq!(api.route("POST", "/api/servers", Some("k3y"), body).status, 202);
        assert_eq!(api.route("POST", "/api/servers", Some("k3y"), b"{}").status, 400);
        let rss = api.route("GET", "/api/rss", Some("k3y"), b"");
        assert!(String::from_utf8_lossy(&rss.body).contains("key=abc"));
        assert_eq!(api.route("POST", "/api/rss", Some("k3y"), br#"["https://x.example/a", " "]"#).status, 202);
        host.tick_local();
        assert_eq!(host.settings.servers.len(), 2);
        assert_eq!(host.settings.servers[0].name, "renamed");
        assert_eq!(host.settings.servers[0].password, "s3cret");
        assert_eq!(host.settings.servers[0].nic, "eth9");
        assert_eq!(host.settings.servers[1].address, "b.example");
        assert_eq!(host.settings.rss_urls, vec!["https://x.example/a".to_string()]);
    }

    #[test]
    fn rows_move_to_a_position_and_expand() {
        let (mut host, api) = demo();
        let first = host.queue[0].id;
        assert_eq!(api.route("POST", &format!("/api/queue/{first}/move?to=2"), Some("k3y"), b"").status, 202);
        assert_eq!(api.route("POST", &format!("/api/queue/{first}/move"), Some("k3y"), b"").status, 400);
        assert_eq!(api.route("POST", &format!("/api/queue/{first}/expand"), Some("k3y"), b"").status, 202);
        assert_eq!(api.route("POST", &format!("/api/queue/{first}/rename"), Some("k3y"), b"New name").status, 202);
        host.tick_local();
        assert_eq!(host.queue[2].id, first);
        assert!(host.queue[2].expanded);
        assert_eq!(host.queue[2].subject, "New name");
        let snap = api.handle.snapshot();
        assert!(!snap.queue[2].file_rows.is_empty());
    }

    /// A one-article NNTP server: greets, accepts any login, answers BODY.
    fn mock_nntp(id: &'static str, body: Vec<u8>) -> u16 {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    let mut lines = BufReader::new(stream);
                    let _ = out.write_all(b"200 mock ready
");
                    let mut line = String::new();
                    while lines.read_line(&mut line).unwrap_or(0) > 0 {
                        let cmd = line.trim().to_string();
                        line.clear();
                        let reply: Vec<u8> = if cmd.starts_with("AUTHINFO") {
                            b"281 ok
".to_vec()
                        } else if cmd == "QUIT" {
                            break;
                        } else if cmd == format!("BODY <{id}>") || cmd == format!("STAT <{id}>") {
                            if cmd.starts_with("STAT") {
                                b"223 0 ok
".to_vec()
                            } else {
                                [b"222 0 body
".as_slice(), &body, b"
.
"].concat()
                            }
                        } else if cmd.starts_with("BODY") || cmd.starts_with("STAT") {
                            b"430 no such article
".to_vec()
                        } else {
                            b"200 ok
".to_vec()
                        };
                        if out.write_all(&reply).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        port
    }

    fn yenc(data: &[u8], name: &str) -> Vec<u8> {
        let mut out = format!("=ybegin line=128 size={} name={name}
", data.len()).into_bytes();
        for b in data {
            let e = b.wrapping_add(42);
            if matches!(e, 0 | 10 | 13 | 61) {
                out.extend([b'=', e.wrapping_add(64)]);
            } else {
                out.push(e);
            }
        }
        out.extend(format!("
=yend size={}
", data.len()).into_bytes());
        out
    }

    /// GUI_PLAN Phase 1 testing: add an NZB through the API, download it
    /// from a mock server, find it in `/api/history`.
    #[test]
    fn an_uploaded_nzb_downloads_into_the_history() {
        let root = std::env::temp_dir().join(format!("nq-api-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let data: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
        let port = mock_nntp("pay1@mock", yenc(&data, "payload.bin"));
        let mut settings = Settings::default();
        settings.servers = vec![newsqueue_core::model::Server {
            name: "mock".into(),
            address: "127.0.0.1".into(),
            port,
            connections: 2,
            ssl: false,
            auth: false,
            enabled: true,
            ..Default::default()
        }];
        settings.download_dir = root.to_string_lossy().to_string();
        settings.temp_dir = root.join("temp").to_string_lossy().to_string();
        settings.low_free_space_mb = 0;
        settings.par2_checking = false;
        settings.auto_unrar = false;
        settings.duplicate_check = 0;
        // Demo mode: nothing is saved over the real settings files.
        let mut engine = Engine::new(settings, true);
        engine.queue.clear();
        engine.history.clear();
        let mut host = EngineHost::new(engine);
        let api = Api { handle: host.handle(), api_key: Some("k3y".into()), web_dir: None };
        let nzb = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<nzb xmlns="http://www.newzbin.com/DTD/2003/nzb">
<file poster="tester" date="1763400000" subject="Api.Test.Release [1/1] - &quot;payload.bin&quot; yEnc (1/1)">
<groups><group>alt.binaries.test</group></groups>
<segments><segment bytes="{}" number="1">pay1@mock</segment></segments>
</file>
</nzb>"#,
            data.len()
        );
        let added = api.route("POST", "/api/nzb?name=Api.Test.Release.nzb", Some("k3y"), nzb.as_bytes());
        assert_eq!(added.status, 202, "{}", String::from_utf8_lossy(&added.body));
        let saved = serde_json::from_slice::<serde_json::Value>(&added.body).unwrap()["file"].as_str().unwrap().to_string();
        assert_eq!(api.route("POST", "/api/connect", Some("k3y"), b"").status, 202);
        let started = std::time::Instant::now();
        let mut history = serde_json::Value::Null;
        while started.elapsed() < Duration::from_secs(20) {
            host.tick_local();
            history = serde_json::from_slice(&api.route("GET", "/api/history", Some("k3y"), b"").body).unwrap();
            if history.as_array().is_some_and(|h| !h.is_empty()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_file(app_base().join("nzb").join("uploads").join(&saved));
        let entry = &history[0];
        assert_eq!(entry["status"], "Completed", "{history}");
        let found = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .flat_map(|d| if d.path().is_dir() { std::fs::read_dir(d.path()).unwrap().flatten().collect() } else { vec![d] })
            .find(|f| f.file_name() == "payload.bin")
            .expect("payload.bin downloaded");
        assert_eq!(std::fs::read(found.path()).unwrap(), data);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn query_values_parse() {
        assert_eq!(query_value("a=1&tag=3", "tag").as_deref(), Some("3"));
        assert_eq!(query_value("paused", "paused").as_deref(), Some(""));
        assert_eq!(query_value("name=test%20one+x.nzb", "name").as_deref(), Some("test one x.nzb"));
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%C3%A9%e"), "é%e");
        assert_eq!(percent_decode("é%"), "é%");
        assert_eq!(query_value("", "x"), None);
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
        assert_eq!(cookie_key("a=1; nq_key=k%33y; b=2").as_deref(), Some("k3y"));
        assert_eq!(cookie_key("a=1"), None);
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
    }

    #[test]
    fn settings_are_public_and_private_keys_stay_put() {
        let (mut host, api) = demo();
        host.with(|e| {
            e.settings.servers[0].password = "s3cret".into();
            e.settings.notify_url = "https://ntfy.example/private".into();
        });
        host.tick_local();
        let r = api.route("GET", "/api/settings", Some("k3y"), b"");
        let text = String::from_utf8(r.body).unwrap();
        assert!(text.contains("direct_unpack"));
        for hidden in ["s3cret", "servers", "notify_url", "proxy_password", "rss_urls"] {
            assert!(!text.contains(hidden), "{hidden} leaked");
        }
        let state = String::from_utf8(api.route("GET", "/api/state", Some("k3y"), b"").body).unwrap();
        assert!(!state.contains("s3cret") && !state.contains("ntfy.example"));

        let post = |body: &str| api.route("POST", "/api/settings", Some("k3y"), body.as_bytes()).status;
        assert_eq!(post(r#"{"servers":[]}"#), 400);
        assert_eq!(post(r#"{"no_such_setting":1}"#), 400);
        assert_eq!(post(r#"{"direct_unpack":"yes"}"#), 400);
        assert_eq!(post("[1]"), 400);
        assert_eq!(post(r#"{"direct_unpack":true,"duplicate_check":2}"#), 202);
        host.tick_local();
        host.with(|e| {
            assert!(e.settings.direct_unpack);
            assert_eq!(e.settings.duplicate_check, 2);
            assert_eq!(e.settings.servers[0].password, "s3cret");
        });
    }

    #[test]
    fn history_and_retry_routes() {
        let (mut host, api) = demo();
        host.with(|e| {
            e.history.clear();
            e.add_history("old".into(), 1, "Failed", String::new());
        });
        host.tick_local();
        let body = String::from_utf8(api.route("GET", "/api/history", Some("k3y"), b"").body).unwrap();
        assert!(body.contains("\"old\"") && body.contains("Failed"));
        assert_eq!(api.route("POST", "/api/history/0/retry", Some("k3y"), b"").status, 202);
        assert_eq!(api.route("POST", "/api/history/5/retry", Some("k3y"), b"").status, 404);
        assert_eq!(api.route("POST", "/api/history/x/retry", Some("k3y"), b"").status, 400);
        assert_eq!(api.route("GET", "/api/log", Some("k3y"), b"").status, 200);
        for path in ["/api/pause-all", "/api/resume-all", "/api/pause-for?minutes=5", "/api/rss/timer?on=1", "/api/par2/0/start"] {
            assert_eq!(api.route("POST", path, Some("k3y"), b"").status, 202, "{path}");
        }
        assert_eq!(api.route("POST", "/api/pause-for", Some("k3y"), b"").status, 400);
        assert_eq!(api.route("POST", "/api/par2/99/start", Some("k3y"), b"").status, 404);
    }

    #[test]
    fn event_stream_sends_changes_only() {
        let (mut host, api) = demo();
        host.tick_local();
        let mut events = EventStream::new(api.handle.clone());
        let first = String::from_utf8(events.next_message().unwrap()).unwrap();
        assert!(first.starts_with("data: {") && first.ends_with("\n\n"));
        // The snapshot is only republished by a tick; without one nothing
        // is sent again. (Ages and session times follow the clock, so a tick
        // may be a real change and is not tested for silence.)
        assert!(events.next_message().is_none(), "unchanged snapshot sent again");
        host.with(|e| e.log("Main", "something new"));
        host.tick_local();
        let next = String::from_utf8(events.next_message().unwrap()).unwrap();
        assert!(next.contains("something new"));
    }

    #[test]
    fn nzb_by_url_is_fetched_and_queued() {
        use std::io::{BufRead, BufReader, Write};
        let config = std::env::temp_dir().join(format!("nq-server-url-{}", std::process::id()));
        std::env::set_var("NQ_CONFIG", &config);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for status in ["200 OK", "404 Not Found"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let nzb = r#"<?xml version="1.0"?><nzb xmlns="http://www.newzbin.com/DTD/2003/nzb"><file poster="p" date="1" subject="x.rar (1/1)"><groups><group>a.b</group></groups><segments><segment bytes="10" number="1">x@y</segment></segments></file></nzb>"#;
                let body = if status.starts_with("200") { nzb } else { "" };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Disposition: attachment; filename=\"Fetched.nzb\"\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let (_host, api) = demo();
        let r = api.route("POST", &format!("/api/nzb/url?url=http://127.0.0.1:{port}/get%3Fkey%3Dsecret"), Some("k3y"), b"");
        let text = String::from_utf8(r.body).unwrap();
        assert_eq!(r.status, 202, "{text}");
        assert!(text.contains("Fetched.nzb"));
        let r = api.route("POST", &format!("/api/nzb/url?url=http://127.0.0.1:{port}/get%3Fkey%3Dsecret"), Some("k3y"), b"");
        let text = String::from_utf8(r.body).unwrap();
        assert_eq!(r.status, 502);
        assert!(!text.contains("secret"), "the URL is not echoed");
        assert_eq!(api.route("POST", "/api/nzb/url?url=file:///etc/passwd", Some("k3y"), b"").status, 400);
        let _ = std::fs::remove_dir_all(config);
    }
}

/// Serves `api` on `addr` from a background thread (one thread per request).
pub fn spawn(addr: SocketAddr, api: Api) -> Result<(), String> {
    let server = tiny_http::Server::http(addr).map_err(|e| format!("cannot listen on {addr}: {e}"))?;
    std::thread::Builder::new()
        .name("http".into())
        .spawn(move || {
            // One thread per request: an event stream stays open for long.
            for request in server.incoming_requests() {
                let api = api.clone();
                std::thread::spawn(move || api.serve(request));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
