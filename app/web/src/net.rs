//! Talking to newsqueue-server: the snapshot (SSE, with polling as the
//! fallback), actions, NZB uploads and the settings endpoints.

use serde::Deserialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct FileRow {
    pub name: String,
    pub bytes: u64,
    pub progress: f32,
    pub held: bool,
    pub done: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueueRow {
    pub id: u64,
    pub subject: String,
    pub status: Option<String>,
    pub total_bytes: u64,
    pub remaining_bytes: f64,
    pub progress: f32,
    pub files: usize,
    pub group: String,
    pub folder: String,
    pub unrar: bool,
    pub has_password: bool,
    pub expanded: bool,
    pub age_days: f64,
    pub parts: u32,
    pub poster: String,
    pub file_rows: Vec<FileRow>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Par2Row {
    pub name: String,
    pub size_mb: f64,
    pub blocks: u32,
    pub info: String,
    pub unrar: bool,
    pub progress: f32,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RssRow {
    pub name: String,
    pub date: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ThreadRow {
    pub number: u32,
    pub action: String,
    pub description: String,
    pub status: String,
    pub server: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Stats {
    pub daily: Vec<f64>,
    pub monthly: Vec<f64>,
    pub session_bytes: u64,
    pub session_secs: u64,
    pub session_collections: u64,
    pub total_collections: u64,
    pub startups: u64,
    pub longest_uptime_secs: u64,
    pub speed_history: Vec<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LogLine {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct HistoryItem {
    pub name: String,
    pub date: String,
    pub size: String,
    pub status: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub seq: u64,
    pub connected: bool,
    pub paused: bool,
    pub speed_bps: f64,
    pub speed_limit_bps: f64,
    pub speed_limit_tag: Option<u8>,
    pub queue_remaining_bytes: f64,
    pub queue: Vec<QueueRow>,
    pub log_tail: Vec<LogLine>,
    pub history: Vec<HistoryItem>,
    pub settings: serde_json::Value,
    pub free_gb: f64,
    pub par2: Vec<Par2Row>,
    pub rss: Vec<RssRow>,
    pub rss_timer: bool,
    pub threads: Vec<ThreadRow>,
    pub stats: Stats,
    pub scheduler_hold: bool,
}

/// What the HTTP callbacks hand back to the UI thread.
#[derive(Default)]
pub struct Shared {
    pub snap: Option<Snapshot>,
    /// Last error from the server (connection, 4xx/5xx).
    pub error: Option<String>,
    pub unauthorized: bool,
    pub in_flight: bool,
    /// The event stream is delivering snapshots: no polling needed.
    pub live: bool,
    /// Messages for the toast line, newest last.
    pub toasts: Vec<String>,
    /// Replies of `GET /api/servers` and `GET /api/rss`.
    pub servers: Option<Vec<serde_json::Value>>,
    pub rss: Option<Vec<String>>,
    pub servers_saving: bool,
    pub servers_saved: Option<Result<Vec<serde_json::Value>, String>>,
}

pub type SharedRef = Arc<Mutex<Shared>>;

pub fn lock(shared: &SharedRef) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn error_text(r: &ehttp::Response) -> String {
    serde_json::from_slice::<serde_json::Value>(&r.bytes)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{} {}", r.status, r.status_text))
}

#[derive(Clone)]
pub struct Client {
    pub shared: SharedRef,
    pub key: String,
}

impl Client {
    pub fn request(&self, method: &str, path: &str, body: Vec<u8>) -> ehttp::Request {
        // Built field by field: `mode` exists only in the WebAssembly build
        // of ehttp, so this also compiles natively (for `cargo test`).
        let mut req = ehttp::Request::get(path);
        req.method = method.into();
        req.body = body;
        req.headers = ehttp::Headers::new(&[]);
        if !self.key.is_empty() {
            req.headers.insert("X-Api-Key", &self.key);
        }
        req
    }

    pub fn poll(&self, ctx: &egui::Context) {
        {
            let mut s = lock(&self.shared);
            if s.in_flight || s.unauthorized {
                return;
            }
            s.in_flight = true;
        }
        let shared = Arc::clone(&self.shared);
        let ctx = ctx.clone();
        ehttp::fetch(self.request("GET", "/api/state", vec![]), move |res| {
            let mut s = lock(&shared);
            s.in_flight = false;
            match res {
                Ok(r) if r.status == 401 => s.unauthorized = true,
                Ok(r) if r.ok => match serde_json::from_slice::<Snapshot>(&r.bytes) {
                    Ok(snap) => {
                        s.snap = Some(snap);
                        s.error = None;
                    }
                    Err(e) => s.error = Some(format!("bad state: {e}")),
                },
                Ok(r) => s.error = Some(format!("server: {}", error_text(&r))),
                Err(e) => s.error = Some(format!("offline: {e}")),
            }
            ctx.request_repaint();
        });
    }

    /// Fire-and-forget action; a failure lands in the toast line.
    pub fn post(&self, ctx: &egui::Context, path: &str, body: Vec<u8>, ok_message: Option<String>) {
        let shared = Arc::clone(&self.shared);
        let ctx2 = ctx.clone();
        ehttp::fetch(self.request("POST", path, body), move |res| {
            let mut s = lock(&shared);
            match res {
                Ok(r) if r.status == 401 => s.unauthorized = true,
                Ok(r) if !r.ok => s.toasts.push(error_text(&r)),
                Err(e) => s.toasts.push(e),
                _ => {
                    if let Some(m) = ok_message {
                        s.toasts.push(m);
                    }
                }
            }
            ctx2.request_repaint();
        });
    }

    /// A 202 only acknowledges queuing. Do not replace the editor with a GET
    /// from before that command was applied (indices also identify passwords).
    pub fn save_servers(&self, ctx: &egui::Context, expected: Vec<serde_json::Value>) {
        { let mut shared=lock(&self.shared); shared.servers_saving=true; shared.servers_saved=None; }
        let client=self.clone(); let ctx=ctx.clone();
        let request=self.request("POST", "/api/servers", serde_json::to_vec(&expected).unwrap());
        ehttp::fetch(request, move |result| {
            match result {
                Ok(response) if response.ok => client.verify_servers(ctx, expected, 30),
                _ => client.finish_servers(&ctx, Err("Could not save servers. Your edits are still available; retry Apply.".into())),
            }
        });
    }

    fn finish_servers(&self, ctx: &egui::Context, result: Result<Vec<serde_json::Value>, String>) {
        let mut shared=lock(&self.shared);
        shared.servers_saving=false;
        shared.toasts.push(match &result { Ok(_) => "Servers saved".into(), Err(message) => message.clone() });
        shared.servers_saved=Some(result);
        ctx.request_repaint();
    }

    fn verify_servers(&self, ctx: egui::Context, expected: Vec<serde_json::Value>, retries: u8) {
        let client=self.clone();
        ehttp::fetch(self.request("GET", "/api/servers", vec![]), move |result| {
            let list=result.ok().filter(|r|r.ok).and_then(|r|serde_json::from_slice::<Vec<serde_json::Value>>(&r.bytes).ok());
            if let Some(list)=list.filter(|list| servers_match(&expected,list)) {
                client.finish_servers(&ctx,Ok(server_indices(list)));
            } else if retries == 0 {
                client.finish_servers(&ctx,Err("Could not confirm the saved servers. Your edits are still available; retry Apply.".into()));
            } else {
                let retry=move ||client.verify_servers(ctx,expected,retries-1);
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    let callback=wasm_bindgen::closure::Closure::once_into_js(retry);
                    if let Some(window)=web_sys::window() {
                        let _=window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(),100);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(100)); retry(); });
            }
        });
    }

    pub fn load_servers(&self, ctx: &egui::Context) {
        let shared = Arc::clone(&self.shared);
        let ctx2 = ctx.clone();
        ehttp::fetch(self.request("GET", "/api/servers", vec![]), move |res| {
            let mut s = lock(&shared);
            match res.ok().filter(|r| r.ok).and_then(|r| serde_json::from_slice::<Vec<serde_json::Value>>(&r.bytes).ok()) {
                Some(list) => s.servers = Some(server_indices(list)),
                None => s.toasts.push("Cannot load the servers".into()),
            }
            ctx2.request_repaint();
        });
    }

    pub fn load_rss(&self, ctx: &egui::Context) {
        let shared = Arc::clone(&self.shared);
        let ctx2 = ctx.clone();
        ehttp::fetch(self.request("GET", "/api/rss", vec![]), move |res| {
            let mut s = lock(&shared);
            match res.ok().filter(|r| r.ok).and_then(|r| serde_json::from_slice::<Vec<String>>(&r.bytes).ok()) {
                Some(list) => s.rss = Some(list),
                None => s.toasts.push("Cannot load the RSS feeds".into()),
            }
            ctx2.request_repaint();
        });
    }

    /// Posts NZB bytes as `name`.
    pub fn upload(&self, ctx: &egui::Context, name: String, bytes: Vec<u8>, paused: bool) {
        let mut path = format!("/api/nzb?name={}", url_encode(&name));
        if paused {
            path.push_str("&paused=1");
        }
        self.post(ctx, &path, bytes, Some(format!("Added {name}")));
    }

    /// Reads a dropped file (asynchronously in the browser) and uploads it.
    pub fn upload_dropped(&self, ctx: &egui::Context, file: Arc<dyn egui::DroppedFile + Send + Sync>, paused: bool) {
        let name = file
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "upload.nzb".into());
        let client = self.clone();
        let ctx = ctx.clone();
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => client.upload(&ctx, name, bytes, paused),
                Err(e) => lock(&client.shared).toasts.push(format!("{name}: {e}")),
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        match file.bytes() {
            Ok(bytes) => client.upload(&ctx, name, bytes, paused),
            Err(e) => lock(&client.shared).toasts.push(format!("{name}: {e}")),
        }
    }

    /// Opens the browser's file picker for .nzb files and uploads each pick.
    pub fn pick_files(&self, ctx: &egui::Context, paused: bool) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use wasm_bindgen::closure::Closure;
            let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
            let Ok(input) = doc.create_element("input").map(|e| e.unchecked_into::<web_sys::HtmlInputElement>()) else {
                return;
            };
            input.set_type("file");
            input.set_accept(".nzb,application/x-nzb");
            input.set_multiple(true);
            let client = self.clone();
            let ctx = ctx.clone();
            let picked = input.clone();
            let on_change = Closure::<dyn FnMut()>::new(move || {
                let Some(files) = picked.files() else { return };
                for i in 0..files.length() {
                    let Some(file) = files.get(i) else { continue };
                    let client = client.clone();
                    let ctx = ctx.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let name = file.name();
                        match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                            Ok(buf) => client.upload(&ctx, name, js_sys::Uint8Array::new(&buf).to_vec(), paused),
                            Err(_) => lock(&client.shared).toasts.push(format!("{name}: cannot read the file")),
                        }
                    });
                }
            });
            input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
            on_change.forget();
            input.click();
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (ctx, paused);
    }
}

/// The `/api/events` stream. Browsers reconnect an EventSource by
/// themselves; a closed one (wrong key, server gone) is reopened by the app.
#[cfg(target_arch = "wasm32")]
pub struct Events {
    source: web_sys::EventSource,
}

#[cfg(target_arch = "wasm32")]
impl Events {
    pub fn open(client: &Client, ctx: &egui::Context) -> Option<Self> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let url = if client.key.is_empty() {
            "/api/events".to_string()
        } else {
            format!("/api/events?apikey={}", url_encode(&client.key))
        };
        let source = web_sys::EventSource::new(&url).ok()?;
        let shared = Arc::clone(&client.shared);
        let ctx2 = ctx.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            let Some(text) = e.data().as_string() else { return };
            if let Ok(snap) = serde_json::from_str::<Snapshot>(&text) {
                let mut s = lock(&shared);
                s.snap = Some(snap);
                s.live = true;
                s.error = None;
            }
            ctx2.request_repaint();
        });
        source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        let shared = Arc::clone(&client.shared);
        let ctx2 = ctx.clone();
        let on_error = Closure::<dyn FnMut()>::new(move || {
            lock(&shared).live = false;
            ctx2.request_repaint();
        });
        source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();
        Some(Self { source })
    }

    pub fn closed(&self) -> bool {
        self.source.ready_state() == web_sys::EventSource::CLOSED
    }

    pub fn close(&self) {
        self.source.close();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Events;

#[cfg(not(target_arch = "wasm32"))]
impl Events {
    pub fn open(_client: &Client, _ctx: &egui::Context) -> Option<Self> {
        None
    }
    pub fn closed(&self) -> bool {
        true
    }
    pub fn close(&self) {}
}

/// Older servers omitted `index`; their GET order is the original identity.
fn server_indices(mut list: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    for (index,server) in list.iter_mut().enumerate() {
        if server.get("index").is_none() { server["index"]=index.into(); }
    }
    list
}

/// Match user-editable fields, excluding secret/index/runtime metadata.
fn servers_match(expected: &[serde_json::Value], actual: &[serde_json::Value]) -> bool {
    expected.len()==actual.len() && expected.iter().zip(actual).all(|(e,a)| {
        e.as_object().is_some_and(|fields|fields.iter().all(|(key,value)| {
            matches!(key.as_str(),"index"|"password"|"password_set"|"used_bytes") || a.get(key)==Some(value)
        }))
    })
}
#[cfg(test)]
mod save_tests {
    use super::*;
    #[test]
    fn older_server_replies_get_stable_editor_indices() {
        let list=server_indices(vec![serde_json::json!({"name":"first"}),serde_json::json!({"name":"second"})]);
        assert_eq!(list[0]["index"],0);
        assert_eq!(list[1]["index"],1);
    }
    #[test]
    fn server_save_rejects_stale_lists_and_accepts_reindexed_redacted_results() {
        let old=serde_json::json!({"index":1,"name":"backup","address":"127.0.0.1","password":"new-secret","password_set":false});
        let stale=serde_json::json!({"index":0,"name":"old","address":"127.0.0.1","password_set":true});
        let saved=serde_json::json!({"index":0,"name":"backup","address":"127.0.0.1","password_set":true});
        assert!(!servers_match(&[old.clone()],&[stale]));
        assert!(!servers_match(&[old.clone()],&[]));
        assert!(servers_match(&[old],&[saved]));
    }
}
