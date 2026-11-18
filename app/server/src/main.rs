//! NewsQueue headless server (GUI_PLAN Phase 1, first slice).
//!
//! Runs the engine on its own thread and serves a JSON API with `tiny_http`
//! (blocking, no async runtime). Front ends change the engine only through
//! `Command`s and read it only through the secret-free `Snapshot`.
//!
//! ```text
//! newsqueue-server [--bind 127.0.0.1:8480] [--config DIR] [--api-key KEY]
//! ```
//!
//! `--config` (or `NQ_CONFIG`) is where settings, queue, history, stats and
//! logs live. `--api-key` (or `NQ_API_KEY`) is required unless the server is
//! bound to a loopback address. There is no built-in TLS: put a reverse proxy
//! in front of it if it is reachable from other machines.

use newsqueue_core::command::{Command, MoveDir};
use newsqueue_core::engine::{Engine, Event};
use newsqueue_core::host::{EngineHost, TICK_INTERVAL};
use newsqueue_core::model::{app_base, parse_nzb, Settings, VERSION};
use newsqueue_core::snapshot::EngineHandle;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_BIND: &str = "127.0.0.1:8480";
/// Largest NZB accepted by `POST /api/nzb`.
pub const MAX_NZB_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Default)]
struct Args {
    bind: Option<String>,
    config: Option<String>,
    api_key: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--bind" => args.bind = Some(value("--bind")?),
            "--config" => args.config = Some(value("--config")?),
            "--api-key" => args.api_key = Some(value("--api-key")?),
            "-h" | "--help" => {
                println!("newsqueue-server [--bind {DEFAULT_BIND}] [--config DIR] [--api-key KEY]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Some(dir) = &args.config {
        std::fs::create_dir_all(dir).ok();
        std::env::set_var("NQ_CONFIG", dir);
    }
    let bind = args
        .bind
        .or_else(|| std::env::var("NQ_BIND").ok())
        .unwrap_or_else(|| DEFAULT_BIND.into());
    let api_key = args
        .api_key
        .or_else(|| std::env::var("NQ_API_KEY").ok())
        .filter(|k| !k.is_empty());
    let addr: SocketAddr = match bind.parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("bad --bind {bind}: {e}");
            std::process::exit(2);
        }
    };
    if api_key.is_none() && !addr.ip().is_loopback() {
        eprintln!("Refusing to listen on {addr} without an API key (--api-key or NQ_API_KEY).");
        std::process::exit(2);
    }

    let mut host = EngineHost::new(Engine::new(Settings::load(), false));
    host.log("Main", &format!("NewsQueue server {VERSION} on {addr}"));
    host.start(TICK_INTERVAL);
    host.checkin();
    let handle = host.handle();

    let server = match tiny_http::Server::http(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot listen on {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("NewsQueue server {VERSION} listening on http://{addr} (config {})", app_base().display());

    let api = Api { handle, api_key };
    std::thread::Builder::new()
        .name("http".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                api.serve(request);
            }
        })
        .expect("spawn http thread");

    // Nobody shows the engine's UI events here; log them so they are not lost.
    loop {
        std::thread::sleep(Duration::from_millis(500));
        for event in host.with(|e| e.take_events()) {
            let line = match event {
                Event::Toast(t) => t,
                Event::Message(title, text) => format!("{title}: {text}"),
                Event::Notify(title, text) => format!("{title}: {text}"),
                Event::PasswordNeeded(id) => format!(
                    "Collection {id} needs an archive password: POST /api/queue/{id}/password"
                ),
                Event::RssFeedsNeeded => "RSS refresh asked for, but no feeds are set".into(),
            };
            println!("{line}");
        }
    }
}

pub struct Api {
    pub handle: EngineHandle,
    pub api_key: Option<String>,
}

pub struct Reply {
    pub status: u16,
    pub body: String,
    pub content_type: &'static str,
}

/// The web UI: one self-contained page that talks to the JSON API.
pub const INDEX_HTML: &str = include_str!("index.html");

fn json(status: u16, value: serde_json::Value) -> Reply {
    Reply { status, body: value.to_string(), content_type: "application/json" }
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

fn query_value(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| percent_decode(v))
}

impl Api {
    fn serve(&self, mut request: tiny_http::Request) {
        let method = request.method().as_str().to_ascii_uppercase();
        let url = request.url().to_string();
        let header_key = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-Api-Key"))
            .map(|h| h.value.as_str().to_string());
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
        let response = tiny_http::Response::from_string(reply.body)
            .with_status_code(reply.status)
            .with_header(content_type);
        let _ = request.respond(response);
    }

    /// Routes one request. Pure apart from sending commands, so it is tested
    /// without a socket.
    pub fn route(&self, method: &str, url: &str, header_key: Option<&str>, body: &[u8]) -> Reply {
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        // The page holds no data; it asks for the API key and sends it itself.
        if method == "GET" && (path == "/" || path == "/index.html") {
            return Reply { status: 200, body: INDEX_HTML.into(), content_type: "text/html; charset=utf-8" };
        }
        if path == "/api/health" {
            return json(200, serde_json::json!({ "ok": true, "version": VERSION }));
        }
        if let Some(expected) = &self.api_key {
            let given = header_key
                .map(str::to_string)
                .or_else(|| query_value(query, "apikey"))
                .unwrap_or_default();
            if !same_secret(&given, expected) {
                return error(401, "missing or wrong API key");
            }
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
            ("POST", ["api", "speed-limit"]) => match query_value(query, "tag").as_deref() {
                None | Some("") | Some("none") => send(Command::SetSpeedLimit(None)),
                Some(tag) => match tag.parse::<u8>() {
                    Ok(t) => send(Command::SetSpeedLimit(Some(t))),
                    Err(_) => error(400, "tag must be a number or none"),
                },
            },
            ("POST", ["api", "nzb"]) => self.add_nzb(query, body),
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

    /// `POST /api/nzb?name=x.nzb&paused=1` with the NZB XML as the body.
    fn add_nzb(&self, query: &str, body: &[u8]) -> Reply {
        if body.is_empty() {
            return error(400, "send the NZB file as the request body");
        }
        let name = query_value(query, "name").unwrap_or_else(|| "upload.nzb".into());
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
        let paused = matches!(query_value(query, "paused").as_deref(), Some("1") | Some("true"));
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
        let api = Api { handle: host.handle(), api_key: Some("k3y".into()) };
        (host, api)
    }

    #[test]
    fn health_is_open_and_the_rest_needs_the_key() {
        let (_host, api) = demo();
        assert_eq!(api.route("GET", "/api/health", None, b"").status, 200);
        let page = api.route("GET", "/", None, b"");
        assert_eq!(page.status, 200);
        assert!(page.content_type.starts_with("text/html"));
        assert_eq!(api.route("GET", "/api/state", None, b"").status, 401);
        assert_eq!(api.route("GET", "/api/state", Some("nope"), b"").status, 401);
        assert_eq!(api.route("GET", "/api/state", Some("k3y"), b"").status, 200);
        assert_eq!(api.route("GET", "/api/state?apikey=k3y", None, b"").status, 200);
    }

    #[test]
    fn state_has_the_queue_and_no_secrets() {
        let (_host, api) = demo();
        let r = api.route("GET", "/api/state", Some("k3y"), b"");
        let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert!(v["queue"].as_array().unwrap().len() >= 3);
        assert!(!r.body.contains("\"password\""));
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
    }
}
