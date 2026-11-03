//! Real NNTP downloading: connect, authenticate, fetch articles, undo yEnc.
//!
//! The UI runs one job at a time, but a job uses the server's configured
//! connection count so several articles are fetched in parallel. It reports
//! progress through a small shared snapshot so the egui thread never blocks
//! on the network.

use crate::model::*;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Connecting,
    Downloading,
    Completed,
    Error,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct DownloadSnapshot {
    pub id: u64,
    pub stage: Stage,
    pub bytes: u64,
    pub total: u64,
    pub file_index: usize,
    pub file_bytes: u64,
    pub file_total: u64,
    /// Actual bytes written for every file, so parallel workers can update
    /// the per-file rows without pretending files finish in order.
    pub file_progress: Vec<u64>,
    /// Number of NNTP connections this job uses.
    pub workers: usize,
    pub speed_bps: f64,
    pub message: String,
    pub current_file: String,
}

pub struct ActiveDownload {
    pub id: u64,
    pub snapshot: Arc<Mutex<DownloadSnapshot>>,
    pub pause: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
}

impl Drop for ActiveDownload {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug)]
pub struct DownloadFile {
    pub name: String,
    pub bytes: u64,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug)]
pub struct DownloadJob {
    pub id: u64,
    pub folder: String,
    pub skip_existing: bool,
    pub files: Vec<DownloadFile>,
}

pub fn job_for_collection(c: &Collection, skip_existing: bool) -> DownloadJob {
    DownloadJob {
        id: c.id,
        folder: c.folder.clone(),
        skip_existing,
        files: c
            .files
            .iter()
            .map(|f| DownloadFile {
                name: f.name.clone(),
                bytes: f.bytes,
                segments: f.segments.clone(),
            })
            .collect(),
    }
}

/// One article that still needs fetching. Articles from different files may
/// be fetched at the same time by different workers.
struct Task {
    file_index: usize,
    path: PathBuf,
    offset: u64,
    id: String,
    /// The first article of a file is fetched during the name lookup pass, so
    /// the worker pool can reuse it instead of downloading it twice.
    prefetched: Option<(Option<u64>, Vec<u8>)>,
}

struct SpeedMeter {
    window: Instant,
    bytes: u64,
}

fn job_total(job: &DownloadJob) -> u64 {
    job.files
        .iter()
        .filter(|f| !f.segments.is_empty())
        .map(|f| f.bytes)
        .sum()
}

pub fn spawn(job: DownloadJob, server: Server, throttle_bps: f64) -> ActiveDownload {
    let id = job.id;
    let total = job_total(&job);
    let planned_workers = (server.connections.max(1) as usize)
        .min(MAX_CONNECTIONS_PER_SERVER as usize);
    let snapshot = Arc::new(Mutex::new(DownloadSnapshot {
        id,
        stage: Stage::Connecting,
        bytes: 0,
        total,
        file_index: 0,
        file_bytes: 0,
        file_total: 0,
        file_progress: vec![0; job.files.len()],
        workers: planned_workers,
        speed_bps: 0.0,
        message: "Connecting".into(),
        current_file: String::new(),
    }));
    let pause = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let snap = snapshot.clone();
    let pause2 = pause.clone();
    let stop2 = stop.clone();
    thread::spawn(move || {
        let result = run_job(&job, &server, throttle_bps, &pause2, &stop2, &snap);
        if let Err(e) = result {
            if let Ok(mut s) = snap.lock() {
                if s.stage != Stage::Stopped {
                    s.stage = Stage::Error;
                    s.speed_bps = 0.0;
                    s.message = e;
                }
            }
        }
    });
    ActiveDownload {
        id,
        snapshot,
        pause,
        stop,
    }
}

fn snapshot_set(snap: &Mutex<DownloadSnapshot>, f: impl FnOnce(&mut DownloadSnapshot)) {
    if let Ok(mut s) = snap.lock() {
        f(&mut s);
    }
}

fn run_job(
    job: &DownloadJob,
    server: &Server,
    throttle_bps: f64,
    pause: &Arc<AtomicBool>,
    stop: &Arc<AtomicBool>,
    snap: &Arc<Mutex<DownloadSnapshot>>,
) -> Result<(), String> {
    if stop.load(Ordering::Relaxed) {
        snapshot_set(snap, |s| s.stage = Stage::Stopped);
        return Ok(());
    }
    fs::create_dir_all(&job.folder).map_err(|e| format!("Cannot create folder: {e}"))?;
    snapshot_set(snap, |s| {
        s.stage = Stage::Connecting;
        s.message = format!("Connecting to {}", host_only(&server.address));
    });

    // Name lookup pass: one connection fetches the first article of every
    // file so the real yEnc file names are known before the parallel workers
    // create the files. The fetched article is kept and reused by a worker.
    let total = job_total(job);
    let mut lookup = Nntp::connect(server)?;
    let mut tasks: VecDeque<Task> = VecDeque::new();
    let mut paths: Vec<Option<PathBuf>> = vec![None; job.files.len()];
    let mut skipped = vec![false; job.files.len()];
    let mut done_bytes: u64 = 0;

    for (index, file) in job.files.iter().enumerate() {
        while pause.load(Ordering::Relaxed) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if stop.load(Ordering::Relaxed) {
            let _ = lookup.quit();
            snapshot_set(snap, |s| s.stage = Stage::Stopped);
            return Ok(());
        }
        let mut segments = file.segments.clone();
        if segments.is_empty() {
            continue;
        }
        segments.sort_by_key(|s| s.number);
        let fallback = format!("file{:02}.bin", index + 1);
        let base = safe_file_name(&file.name, &fallback);
        let body = match lookup.body(&segments[0].id) {
            Ok(body) => body,
            Err(e) => {
                let _ = lookup.quit();
                return Err(e);
            }
        };
        let (yenc_name, first_begin, first_data) = match decode_yenc(&body) {
            Ok(decoded) => decoded,
            Err(e) => {
                let _ = lookup.quit();
                return Err(e);
            }
        };
        let name = if yenc_name.is_empty() {
            base
        } else {
            safe_file_name(&yenc_name, &base)
        };
        let path = PathBuf::from(&job.folder).join(&name);
        paths[index] = Some(path.clone());
        if job.skip_existing
            && fs::metadata(&path)
                .map(|m| m.len() >= file.bytes)
                .unwrap_or(false)
        {
            skipped[index] = true;
            done_bytes = done_bytes.saturating_add(file.bytes);
            snapshot_set(snap, |s| {
                s.bytes = done_bytes;
                if let Some(p) = s.file_progress.get_mut(index) {
                    *p = file.bytes;
                }
            });
            continue;
        }
        // A previous interrupted attempt can leave a shorter file behind.
        // Start this file from scratch instead of mixing old and new bytes.
        if let Err(e) = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
        {
            let _ = lookup.quit();
            return Err(format!("Cannot create file: {e}"));
        }
        let mut offset = 0u64;
        for (seg_index, segment) in segments.iter().enumerate() {
            let prefetched = if seg_index == 0 {
                Some((first_begin, first_data.clone()))
            } else {
                None
            };
            tasks.push_back(Task {
                file_index: index,
                path: path.clone(),
                offset,
                id: segment.id.clone(),
                prefetched,
            });
            offset = offset.saturating_add(segment.bytes);
        }
    }
    let _ = lookup.quit();

    if tasks.is_empty() {
        let stage = if stop.load(Ordering::Relaxed) {
            Stage::Stopped
        } else {
            Stage::Completed
        };
        snapshot_set(snap, |s| {
            s.stage = stage;
            s.bytes = total;
            s.total = total;
            s.file_index = job.files.len();
            s.file_bytes = 0;
            s.speed_bps = 0.0;
            s.message = if stage == Stage::Completed {
                "Download complete".into()
            } else {
                "Stopped".into()
            };
        });
        return Ok(());
    }

    let worker_count = (server.connections.max(1) as usize)
        .min(tasks.len())
        .min(MAX_CONNECTIONS_PER_SERVER as usize);
    let per_worker_throttle = if throttle_bps > 0.0 {
        throttle_bps / worker_count as f64
    } else {
        0.0
    };
    snapshot_set(snap, |s| {
        s.stage = Stage::Downloading;
        s.bytes = done_bytes;
        s.workers = worker_count;
        s.message = format!("Downloading with {worker_count} connections");
    });

    let shared_tasks = Arc::new(Mutex::new(tasks));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let connect_errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let speed = Arc::new(Mutex::new(SpeedMeter {
        window: Instant::now(),
        bytes: 0,
    }));
    let server = server.clone();
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let server = server.clone();
        let tasks = shared_tasks.clone();
        let errors = errors.clone();
        let connect_errors = connect_errors.clone();
        let speed = speed.clone();
        let pause = Arc::clone(pause);
        let stop = Arc::clone(stop);
        let snap = Arc::clone(snap);
        handles.push(thread::spawn(move || {
            download_worker(
                &server,
                &tasks,
                &pause,
                &stop,
                &snap,
                &speed,
                &errors,
                &connect_errors,
                per_worker_throttle,
            )
        }));
    }
    for handle in handles {
        if handle.join().is_err() && errors.lock().map(|e| e.is_empty()).unwrap_or(false) {
            if let Ok(mut e) = errors.lock() {
                e.push("A download worker stopped unexpectedly".into());
            }
        }
    }

    let fatal = errors.lock().ok().and_then(|e| e.first().cloned());
    if let Some(message) = fatal {
        snapshot_set(snap, |s| {
            s.stage = Stage::Error;
            s.speed_bps = 0.0;
            s.message = message.clone();
        });
        return Err(message);
    }
    if stop.load(Ordering::Relaxed) {
        snapshot_set(snap, |s| {
            s.stage = Stage::Stopped;
            s.speed_bps = 0.0;
        });
        return Ok(());
    }
    let remaining = shared_tasks.lock().map(|t| t.len()).unwrap_or(0);
    if remaining > 0 {
        let message = connect_errors
            .lock()
            .ok()
            .and_then(|e| e.first().cloned())
            .unwrap_or_else(|| format!("{remaining} articles were not downloaded"));
        snapshot_set(snap, |s| {
            s.stage = Stage::Error;
            s.speed_bps = 0.0;
            s.message = message.clone();
        });
        return Err(message);
    }

    for (index, file) in job.files.iter().enumerate() {
        if skipped[index] || file.segments.is_empty() {
            continue;
        }
        if let Some(path) = &paths[index] {
            let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            if len != file.bytes {
                let message = format!(
                    "{} is incomplete: expected {} bytes but wrote {}",
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    file.bytes,
                    len
                );
                snapshot_set(snap, |s| {
                    s.stage = Stage::Error;
                    s.speed_bps = 0.0;
                    s.message = message.clone();
                });
                return Err(message);
            }
        }
    }

    snapshot_set(snap, |s| {
        s.stage = Stage::Completed;
        s.bytes = total;
        s.total = total;
        s.file_index = job.files.len();
        s.file_bytes = 0;
        s.speed_bps = 0.0;
        s.message = "Download complete".into();
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn download_worker(
    server: &Server,
    tasks: &Mutex<VecDeque<Task>>,
    pause: &AtomicBool,
    stop: &AtomicBool,
    snap: &Mutex<DownloadSnapshot>,
    speed: &Mutex<SpeedMeter>,
    errors: &Mutex<Vec<String>>,
    connect_errors: &Mutex<Vec<String>>,
    throttle_bps: f64,
) {
    let mut nntp = match Nntp::connect(server) {
        Ok(nntp) => nntp,
        Err(e) => {
            // One failed connection should not kill the whole job; the
            // remaining workers can still finish the queue.
            if let Ok(mut list) = connect_errors.lock() {
                list.push(e);
            }
            return;
        }
    };
    let mut files: HashMap<usize, fs::File> = HashMap::new();
    let mut next_allowed = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        while pause.load(Ordering::Relaxed) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let task = {
            match tasks.lock() {
                Ok(mut queue) => queue.pop_front(),
                Err(_) => None,
            }
        };
        let Some(mut task) = task else { break };
        let decoded = match task.prefetched.take() {
            Some(value) => Ok(value),
            None => nntp
                .body(&task.id)
                .and_then(|body| decode_yenc(&body).map(|(_, begin, data)| (begin, data))),
        };
        let (begin, data) = match decoded {
            Ok(value) => value,
            Err(e) => {
                record_error(errors, stop, e);
                break;
            }
        };
        let offset = begin
            .map(|value| value.saturating_sub(1))
            .unwrap_or(task.offset);
        let write_result = write_article(&mut files, &task, offset, &data);
        if let Err(e) = write_result {
            record_error(errors, stop, e);
            break;
        }
        let chunk = data.len() as u64;

        let mut measured = None;
        if let Ok(mut meter) = speed.lock() {
            meter.bytes = meter.bytes.saturating_add(chunk);
            let elapsed = meter.window.elapsed();
            if elapsed >= Duration::from_millis(500) {
                measured = Some(meter.bytes as f64 / elapsed.as_secs_f64());
                meter.bytes = 0;
                meter.window = Instant::now();
            }
        }
        if let Ok(mut s) = snap.lock() {
            s.bytes = s.bytes.saturating_add(chunk);
            s.file_index = task.file_index;
            if let Some(progress) = s.file_progress.get_mut(task.file_index) {
                *progress = progress.saturating_add(chunk);
                s.file_bytes = *progress;
            }
            s.current_file = task
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if let Some(bps) = measured {
                s.speed_bps = bps;
            }
        }

        if throttle_bps > 0.0 {
            let budget = Duration::from_secs_f64(chunk as f64 / throttle_bps);
            let now = Instant::now();
            next_allowed = next_allowed.max(now) + budget;
            if next_allowed > now {
                thread::sleep(next_allowed - now);
            }
        }
    }
    let _ = nntp.quit();
    for file in files.values_mut() {
        let _ = file.flush();
    }
}

fn write_article(
    files: &mut HashMap<usize, fs::File>,
    task: &Task,
    offset: u64,
    data: &[u8],
) -> Result<(), String> {
    if !files.contains_key(&task.file_index) {
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&task.path)
            .map_err(|e| format!("Cannot create file: {e}"))?;
        files.insert(task.file_index, file);
    }
    let file = files
        .get_mut(&task.file_index)
        .ok_or_else(|| "Cannot open output file".to_string())?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Cannot seek in file: {e}"))?;
    file.write_all(data)
        .map_err(|e| format!("Cannot write file: {e}"))
}

fn record_error(errors: &Mutex<Vec<String>>, stop: &AtomicBool, message: String) {
    if let Ok(mut list) = errors.lock() {
        if list.is_empty() {
            list.push(message);
        }
    }
    stop.store(true, Ordering::Relaxed);
}

enum NetStream {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl Read for NetStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for NetStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            Self::Tls(stream) => stream.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

struct Nntp {
    reader: BufReader<NetStream>,
}

impl Nntp {
    fn connect(server: &Server) -> Result<Self, String> {
        let host = host_only(&server.address);
        // Function 0x870648: SSL uses port 563, plain NNTP uses 119.
        let port = if server.port == 0 {
            if server.ssl {
                SSL_PORT
            } else {
                PLAIN_PORT
            }
        } else {
            server.port
        };
        let timeout = Duration::from_secs(server_timeout());
        let addresses = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| format!("Cannot look up {host}: {e}"))?;
        let mut chosen: Option<TcpStream> = None;
        let mut last_error = String::new();
        for address in addresses {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => {
                    chosen = Some(stream);
                    break;
                }
                Err(e) => last_error = e.to_string(),
            }
        }
        let stream = chosen.ok_or_else(|| {
            if last_error.is_empty() {
                format!("Cannot connect to {host}:{port}")
            } else {
                format!("Cannot connect to {host}:{port}: {last_error}")
            }
        })?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        let stream = if server.ssl {
            let connector = native_tls::TlsConnector::builder()
                .build()
                .map_err(|e| format!("Cannot create the TLS connector: {e}"))?;
            let tls = connector
                .connect(&host, stream)
                .map_err(|e| format!("TLS handshake with {host}:{port} failed: {e}"))?;
            NetStream::Tls(tls)
        } else {
            NetStream::Plain(stream)
        };
        let mut nntp = Self {
            reader: BufReader::new(stream),
        };
        let greeting = nntp.read_line()?;
        if !(greeting.starts_with("200") || greeting.starts_with("201")) {
            return Err(format!("Server refused the connection: {}", reply_code(&greeting)));
        }
        if server.auth && !server.username.trim().is_empty() {
            let user = nntp.command(&format!("AUTHINFO USER {}", server.username.trim()))?;
            if user.starts_with("381") {
                let pass = nntp.command(&format!("AUTHINFO PASS {}", server.password))?;
                if !pass.starts_with("281") {
                    return Err(format!("Authentication failed: {}", reply_code(&pass)));
                }
            } else if !user.starts_with("281") {
                return Err(format!("Authentication failed: {}", reply_code(&user)));
            }
        }
        Ok(nntp)
    }

    fn send(&mut self, line: &str) -> Result<(), String> {
        let writer = self.reader.get_mut();
        writer
            .write_all(format!("{line}\r\n").as_bytes())
            .map_err(|e| format!("Cannot send to server: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("Cannot send to server: {e}"))
    }

    fn command(&mut self, line: &str) -> Result<String, String> {
        self.send(line)?;
        self.read_line()
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = Vec::new();
        let n = self
            .reader
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("Cannot read from server: {e}"))?;
        if n == 0 {
            return Err("Server closed the connection".into());
        }
        while matches!(line.last(), Some(&b'\n') | Some(&b'\r')) {
            line.pop();
        }
        Ok(String::from_utf8_lossy(&line).to_string())
    }

    fn body(&mut self, id: &str) -> Result<Vec<u8>, String> {
        let response = self.command(&format!("BODY <{id}>"))?;
        if response.starts_with("430") {
            return Err(format!("Article not found: {id}"));
        }
        if !(response.starts_with("222") || response.starts_with("220")) {
            return Err(format!("Server refused article: {}", reply_code(&response)));
        }
        let mut out = Vec::new();
        loop {
            let mut line = Vec::new();
            let n = self
                .reader
                .read_until(b'\n', &mut line)
                .map_err(|e| format!("Cannot read article: {e}"))?;
            if n == 0 {
                return Err("Server closed the connection during an article".into());
            }
            while matches!(line.last(), Some(&b'\n') | Some(&b'\r')) {
                line.pop();
            }
            if line == b"." {
                break;
            }
            if line.first() == Some(&b'.') {
                line.remove(0);
            }
            out.extend_from_slice(&line);
            out.push(b'\n');
        }
        Ok(out)
    }

    fn quit(&mut self) -> Result<(), String> {
        let _ = self.send("QUIT");
        Ok(())
    }
}

fn reply_code(reply: &str) -> String {
    reply.chars().take(3).collect()
}

fn server_timeout() -> u64 {
    CONNECTION_TIMEOUT.max(5) as u64
}

fn host_only(address: &str) -> String {
    let mut value = address.trim();
    for prefix in ["https://", "http://", "nntps://", "nntp://"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest;
            break;
        }
    }
    let value = value.split('/').next().unwrap_or(value).trim();
    if let Some((host, port)) = value.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return host.to_string();
        }
    }
    value.to_string()
}

fn safe_file_name(name: &str, fallback: &str) -> String {
    let raw = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

fn ybegin_name(line: &[u8]) -> String {
    let text = String::from_utf8_lossy(line);
    if let Some(pos) = text.find("name=") {
        text[pos + 5..].trim().to_string()
    } else {
        String::new()
    }
}

fn ypart_begin(line: &[u8]) -> Option<u64> {
    let text = String::from_utf8_lossy(line);
    for token in text.split_whitespace() {
        if let Some(value) = token.strip_prefix("begin=") {
            return value.parse::<u64>().ok();
        }
    }
    None
}

/// Returns the yEnc name, the 1-based begin offset from `=ypart` when the
/// article carries one, and the decoded bytes.
fn decode_yenc(body: &[u8]) -> Result<(String, Option<u64>, Vec<u8>), String> {
    let mut name = String::new();
    let mut begin = None;
    let mut started = false;
    // Line breaks inside a yEnc body are not part of the data, so join the
    // encoded lines first. An escape sequence may be split across two lines.
    let mut encoded = Vec::with_capacity(body.len());
    for raw in body.split(|b| *b == b'\n') {
        let mut line = raw;
        while line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        if !started {
            if line.starts_with(b"=ybegin") {
                name = ybegin_name(line);
                started = true;
            }
            continue;
        }
        if line.starts_with(b"=yend") {
            break;
        }
        if line.starts_with(b"=ypart") {
            if let Some(value) = ypart_begin(line) {
                begin = Some(value);
            }
            continue;
        }
        encoded.extend_from_slice(line);
    }
    if !started {
        return Err("Article did not contain a yEnc body".into());
    }
    let mut out = Vec::with_capacity(encoded.len());
    let mut i = 0;
    while i < encoded.len() {
        let c = encoded[i];
        if c == b'=' {
            if i + 1 >= encoded.len() {
                return Err("Broken yEnc escape sequence".into());
            }
            out.push(encoded[i + 1].wrapping_sub(64));
            i += 2;
        } else {
            out.push(c.wrapping_sub(42));
            i += 1;
        }
    }
    Ok((name, begin, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_yenc_escapes() {
        // 19 encodes to '=' so it is sent as "=S"; 214 encodes to NUL so it is
        // sent as "=" plus (214 + 64) & 0xff; 0 encodes to '*'.
        let body = b"=ybegin line=128 size=3 name=test.bin\n=S*=\x16\n=yend size=3\n";
        let (name, begin, data) = decode_yenc(body).unwrap();
        assert_eq!(name, "test.bin");
        assert_eq!(begin, None);
        assert_eq!(data, vec![19, 0, 214]);
    }

    #[test]
    fn reads_the_yenc_part_offset() {
        let body = b"=ybegin part=2 total=3 line=128 size=9 name=test.bin\n=ypart begin=4 end=6\n*+,\n=yend size=3\n";
        let (_, begin, data) = decode_yenc(body).unwrap();
        assert_eq!(begin, Some(4));
        assert_eq!(data, vec![0, 1, 2]);
    }

    #[test]
    fn strips_a_nntp_server_url_to_its_host() {
        assert_eq!(host_only("https://news.easynews.com"), "news.easynews.com");
        assert_eq!(host_only("news.easynews.com:563"), "news.easynews.com");
    }
}

/// Console check used by `--download-test`: exercises the same NNTP worker the
/// window uses, without the speed cap, and prints progress.
pub fn run_download_test(path: &str) -> Result<(), String> {
    let settings = Settings::load();
    let parsed = parse_nzb(Path::new(path))?;
    let mut server = settings
        .servers
        .iter()
        .find(|s| s.enabled)
        .cloned()
        .ok_or_else(|| "No enabled server configured in Setup".to_string())?;
    if server.address.contains("example.com") {
        server = Server::default();
    }
    let folder = PathBuf::from(&settings.download_dir).join(sanitize_folder(&parsed.name));
    let job = DownloadJob {
        id: 1,
        folder: folder.to_string_lossy().to_string(),
        skip_existing: false,
        files: parsed
            .files
            .iter()
            .map(|f| DownloadFile {
                name: f.name.clone(),
                bytes: f.bytes,
                segments: f.segments.clone(),
            })
            .collect(),
    };
    let total = job_total(&job);
    println!(
        "Downloading {} ({} bytes) from {}:{} into {}",
        parsed.name,
        total,
        host_only(&server.address),
        server.port,
        folder.display()
    );
    let active = spawn(job, server, 0.0);
    let mut last = 0u64;
    loop {
        thread::sleep(Duration::from_millis(500));
        let snapshot = active
            .snapshot
            .lock()
            .map_err(|_| "Download state lock failed".to_string())?
            .clone();
        if snapshot.bytes != last {
            println!(
                "{}/{} bytes ({:.1}%) {}",
                snapshot.bytes,
                total,
                100.0 * snapshot.bytes as f64 / total.max(1) as f64,
                snapshot.current_file
            );
            last = snapshot.bytes;
        }
        match snapshot.stage {
            Stage::Completed => {
                println!("Download complete: {}", folder.display());
                return Ok(());
            }
            Stage::Error => return Err(snapshot.message),
            Stage::Stopped => return Err("Download stopped".into()),
            _ => {}
        }
    }
}
