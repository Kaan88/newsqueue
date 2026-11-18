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
//!
//! Endpoints (all but `/api/health` and `/api/login` need the key, as the
//! `X-Api-Key` header, `?apikey=` or the `nq_key` cookie from login):
//!
//! ```text
//! GET  /api/health                      version, no key needed
//! POST /api/login        body: key      sets the nq_key cookie
//! GET  /api/state                       queue, speed, log tail, history, public settings
//! GET  /api/events                      the same as Server-Sent Events, on change
//! GET  /api/history | /api/log | /api/settings
//! POST /api/settings     body: JSON     change settings (private keys refused)
//! POST /api/history/N/retry
//! POST /api/pause | resume | connect | disconnect | cleanup
//! POST /api/speed-limit?tag=N|none
//! POST /api/nzb?name=x.nzb&paused=1     body: NZB file
//! POST /api/nzb/url?url=...&paused=1    the server fetches the NZB
//! POST /api/queue/ID/(pause|resume|delete|up|down|top|bottom|unpack|password)
//! ```

use newsqueue_core::engine::{Engine, Event};
use newsqueue_core::host::{EngineHost, TICK_INTERVAL};
use newsqueue_core::model::{app_base, Settings, VERSION};
use newsqueue_server::{spawn, web_dir, Api, DEFAULT_BIND};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Default)]
struct Args {
    bind: Option<String>,
    config: Option<String>,
    api_key: Option<String>,
    web: Option<String>,
    /// Sample queue and history, nothing saved: for looking at the UI.
    demo: bool,
    /// Ask the running server at `--bind`/`NQ_BIND` for /api/health and
    /// exit 0 or 1 (the Docker HEALTHCHECK; no curl in the image).
    health_check: bool,
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
            "--web" => args.web = Some(value("--web")?),
            "--demo" => args.demo = true,
            "--health-check" => args.health_check = true,
            "-h" | "--help" => {
                println!("newsqueue-server [--bind {DEFAULT_BIND}] [--config DIR] [--api-key KEY] [--web DIR] [--demo] [--health-check]");
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
    if args.health_check {
        let host = if addr.ip().is_unspecified() { "127.0.0.1".to_string() } else { addr.ip().to_string() };
        let url = format!("http://{host}:{}/api/health", addr.port());
        let ok = ureq::get(&url).timeout(Duration::from_secs(5)).call().is_ok_and(|r| r.status() == 200);
        std::process::exit(if ok { 0 } else { 1 });
    }
    if api_key.is_none() && !addr.ip().is_loopback() {
        eprintln!("Refusing to listen on {addr} without an API key (--api-key or NQ_API_KEY).");
        std::process::exit(2);
    }

    let mut host = EngineHost::new(Engine::new(if args.demo { Settings::default() } else { Settings::load() }, args.demo));
    host.log("Main", &format!("NewsQueue server {VERSION} on {addr}"));
    host.start(TICK_INTERVAL);
    host.checkin();
    let handle = host.handle();

    println!("NewsQueue server {VERSION} listening on http://{addr} (config {})", app_base().display());

    let web_dir = web_dir(args.web);
    match &web_dir {
        Some(d) => println!("egui web UI from {}", d.display()),
        None => println!("no egui web build found (web/build-web.ps1); serving the HTML page at /"),
    }
    if let Err(e) = spawn(addr, Api { handle, api_key, web_dir }) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    let quit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let quit_flag = quit.clone();
    if let Err(e) = ctrlc::set_handler(move || quit_flag.store(true, std::sync::atomic::Ordering::Relaxed)) {
        eprintln!("no Ctrl+C handler: {e}");
    }

    // Nobody shows the engine's UI events here; log them so they are not lost.
    loop {
        std::thread::sleep(Duration::from_millis(500));
        if quit.load(std::sync::atomic::Ordering::Relaxed) {
            host.with(|e| {
                e.log("Main", "Server stopping (Ctrl+C); state saved.");
                e.save_all();
            });
            println!("Saved settings, queue, history and statistics. Bye.");
            std::process::exit(0);
        }
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

