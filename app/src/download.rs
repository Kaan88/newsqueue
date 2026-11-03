//! Real NNTP downloading: connect, authenticate, fetch articles, undo yEnc.
//!
//! The UI runs one job at a time, but a job uses the server's configured
//! connection count so several articles are fetched in parallel. It reports
//! progress through a small shared snapshot so the egui thread never blocks
//! on the network.

use crate::model::*;
use std::collections::{HashMap, HashSet, VecDeque};
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
    Incomplete,
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
    /// Articles the server reported as unavailable (430).
    pub missing: usize,
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
    pub retry_count: u32,
    pub retry_delay: u32,
    pub priority: u32,
    pub cache_mb: u32,
    pub cache_cleanup_minutes: u32,
    pub proxy: ProxyConfig,
    pub files: Vec<DownloadFile>,
}

pub fn job_for_collection(
    c: &Collection,
    skip_existing: bool,
    retry_count: u32,
    retry_delay: u32,
    priority: u32,
    cache_mb: u32,
    cache_cleanup_minutes: u32,
    proxy: ProxyConfig,
) -> DownloadJob {
    DownloadJob {
        id: c.id,
        folder: c.folder.clone(),
        skip_existing,
        retry_count,
        retry_delay,
        priority,
        cache_mb,
        cache_cleanup_minutes,
        proxy,
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
    progress: PathBuf,
    /// The first article of a file is fetched during the name lookup pass, so
    /// the worker pool can reuse it instead of downloading it twice.
    prefetched: Option<(Option<u64>, Vec<u8>)>,
}

struct SpeedMeter {
    window: Instant,
    bytes: u64,
}

struct CachedArticle {
    body: Vec<u8>,
    stored: Instant,
}

struct ArticleCache {
    limit: usize,
    ttl: Duration,
    bytes: usize,
    entries: HashMap<String, CachedArticle>,
    order: VecDeque<String>,
}

impl ArticleCache {
    fn new(limit_mb: u32, cleanup_minutes: u32) -> Self {
        Self {
            limit: limit_mb as usize * 1024 * 1024,
            ttl: Duration::from_secs(cleanup_minutes.max(1) as u64 * 60),
            bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, id: &str) -> Option<Vec<u8>> {
        if self.limit == 0 {
            return None;
        }
        self.cleanup(Instant::now());
        let entry = self.entries.get(id)?;
        Some(entry.body.clone())
    }

    fn insert(&mut self, id: String, body: Vec<u8>) {
        if self.limit == 0 || body.len() > self.limit {
            return;
        }
        self.cleanup(Instant::now());
        if let Some(previous) = self.entries.remove(&id) {
            self.bytes = self.bytes.saturating_sub(previous.body.len());
            self.order.retain(|key| key != &id);
        }
        while self.bytes + body.len() > self.limit {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.body.len());
            }
        }
        self.bytes += body.len();
        self.order.push_back(id.clone());
        self.entries.insert(
            id,
            CachedArticle {
                body,
                stored: Instant::now(),
            },
        );
    }

    fn cleanup(&mut self, now: Instant) {
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.stored) > self.ttl)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(entry) = self.entries.remove(&id) {
                self.bytes = self.bytes.saturating_sub(entry.body.len());
            }
            self.order.retain(|key| key != &id);
        }
    }
}

fn job_total(job: &DownloadJob) -> u64 {
    job.files
        .iter()
        .filter(|f| !f.segments.is_empty())
        .map(|f| f.bytes)
        .sum()
}

fn progress_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.nqpart", path.to_string_lossy()))
}

pub fn spawn(job: DownloadJob, servers: Vec<Server>, throttle_bps: f64) -> ActiveDownload {
    let id = job.id;
    let total = job_total(&job);
    let planned_workers = servers
        .iter()
        .map(|server| (server.connections.max(1) as usize).min(MAX_CONNECTIONS_PER_SERVER as usize))
        .sum::<usize>()
        .max(1);
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
        missing: 0,
    }));
    let pause = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let snap = snapshot.clone();
    let pause2 = pause.clone();
    let stop2 = stop.clone();
    thread::spawn(move || {
        let result = run_job(&job, &servers, throttle_bps, &pause2, &stop2, &snap);
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
    servers: &[Server],
    throttle_bps: f64,
    pause: &Arc<AtomicBool>,
    stop: &Arc<AtomicBool>,
    snap: &Arc<Mutex<DownloadSnapshot>>,
) -> Result<(), String> {
    let server = servers
        .first()
        .ok_or_else(|| "No enabled NNTP server configured".to_string())?;
    if stop.load(Ordering::Relaxed) {
        snapshot_set(snap, |s| s.stage = Stage::Stopped);
        return Ok(());
    }
    fs::create_dir_all(&job.folder).map_err(|e| format!("Cannot create folder: {e}"))?;
    snapshot_set(snap, |s| {
        s.stage = Stage::Connecting;
        s.message = format!(
            "Connecting to {}",
            servers
                .iter()
                .map(|server| host_only(&server.address))
                .collect::<Vec<_>>()
                .join(", ")
        );
    });

    // Name lookup pass: one connection fetches the first article of every
    // file so the real yEnc file names are known before the parallel workers
    // create the files. The fetched article is kept and reused by a worker.
    let total = job_total(job);
    let cache = Arc::new(Mutex::new(ArticleCache::new(
        job.cache_mb,
        job.cache_cleanup_minutes,
    )));
    let mut lookup = Nntp::connect(server, &job.proxy)?;
    let mut tasks: VecDeque<Task> = VecDeque::new();
    let mut paths: Vec<Option<PathBuf>> = vec![None; job.files.len()];
    let mut skipped = vec![false; job.files.len()];
    let mut expected_sizes: Vec<Option<u64>> = vec![None; job.files.len()];
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
        let body = if let Some(body) = cache.lock().ok().and_then(|mut cache| cache.get(&segments[0].id)) {
            body
        } else {
            match lookup.body(&segments[0].id) {
                Ok(body) => {
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(segments[0].id.clone(), body.clone());
                    }
                    body
                }
                Err(e) => {
                    let _ = lookup.quit();
                    return Err(e);
                }
            }
        };
        let (yenc_name, first_begin, first_data) = match decode_yenc(&body) {
            Ok(decoded) => decoded,
            Err(e) => {
                let _ = lookup.quit();
                return Err(e);
            }
        };
        let decoded_size = yenc_size(&body);
        expected_sizes[index] = decoded_size;
        let name = if yenc_name.is_empty() {
            base
        } else if Path::new(&file.name).extension().is_some() {
            // Obfuscated posts often put a random name in =ybegin. The NZB
            // subject still carries the intended filename and extension.
            base
        } else {
            safe_file_name(&yenc_name, &base)
        };
        let path = PathBuf::from(&job.folder).join(&name);
        paths[index] = Some(path.clone());
        if job.skip_existing
            && decoded_size
                .map(|size| {
                    fs::metadata(&path)
                        .map(|metadata| metadata.len() >= size)
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        {
            skipped[index] = true;
            let _ = fs::remove_file(progress_path_for(&path));
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
        // Keep the bytes only when a sidecar proves which articles were written;
        // otherwise start from scratch so old and new data are never mixed.
        let progress_path = progress_path_for(&path);
        let completed_ids: HashSet<String> = if progress_path.exists() {
            fs::read_to_string(&progress_path)
                .map(|text| {
                    text.lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        let resuming = !completed_ids.is_empty()
            && fs::metadata(&path).map(|meta| meta.len() > 0).unwrap_or(false);
        if resuming {
            let resumed_bytes: u64 = segments
                .iter()
                .filter(|segment| completed_ids.contains(&segment.id))
                .map(|segment| segment.bytes)
                .sum();
            done_bytes = done_bytes.saturating_add(resumed_bytes);
            snapshot_set(snap, |snapshot| {
                snapshot.bytes = done_bytes;
                if let Some(progress) = snapshot.file_progress.get_mut(index) {
                    *progress = resumed_bytes;
                }
            });
        }
        if !resuming {
            let _ = fs::remove_file(&progress_path);
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).write(true);
        if !resuming {
            options.truncate(true);
        }
        if let Err(e) = options.open(&path) {
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
            if !completed_ids.contains(&segment.id) {
                tasks.push_back(Task {
                    file_index: index,
                    path: path.clone(),
                    offset,
                    id: segment.id.clone(),
                    progress: progress_path.clone(),
                    prefetched,
                });
            }
            offset = offset.saturating_add(segment.bytes);
        }
        if completed_ids.len() >= segments.len() {
            let _ = fs::remove_file(&progress_path);
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

    let total_connections = servers
        .iter()
        .map(|server| (server.connections.max(1) as usize).min(MAX_CONNECTIONS_PER_SERVER as usize))
        .sum::<usize>()
        .max(1);
    let worker_count = total_connections.min(tasks.len()).max(1);
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
    let servers = servers.to_vec();
    let retry_count = job.retry_count;
    let retry_delay = job.retry_delay;
    let priority = job.priority;
    let proxy = job.proxy.clone();
    let mut handles = Vec::new();
    for worker_index in 0..worker_count {
        let server = servers[worker_index % servers.len()].clone();
        let tasks = shared_tasks.clone();
        let errors = errors.clone();
        let connect_errors = connect_errors.clone();
        let speed = speed.clone();
        let pause = Arc::clone(pause);
        let stop = Arc::clone(stop);
        let snap = Arc::clone(snap);
        let cache = cache.clone();
        let proxy = proxy.clone();
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
                &cache,
                &proxy,
                per_worker_throttle,
                retry_count,
                retry_delay,
                priority,
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

    let missing = snap.lock().map(|snapshot| snapshot.missing).unwrap_or(0);
    if missing > 0 {
        snapshot_set(snap, |s| {
            s.stage = Stage::Incomplete;
            s.bytes = total;
            s.total = total;
            s.file_index = job.files.len();
            s.file_bytes = 0;
            s.speed_bps = 0.0;
            s.message = format!("{missing} article(s) missing; PAR2 repair required");
        });
        return Ok(());
    }

    let mut bad_size = None;
    for (index, expected) in expected_sizes.iter().enumerate() {
        let Some(expected) = expected else { continue };
        let Some(path) = &paths[index] else { continue };
        let actual = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        if actual != *expected {
            bad_size = Some(format!(
                "{} has {} bytes, expected {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                actual,
                expected
            ));
            break;
        }
    }
    if let Some(message) = bad_size {
        snapshot_set(snap, |s| {
            s.stage = Stage::Incomplete;
            s.bytes = total;
            s.total = total;
            s.file_index = job.files.len();
            s.file_bytes = 0;
            s.speed_bps = 0.0;
            s.message = format!("{message}; PAR2 repair required");
        });
        return Ok(());
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
    for path in paths.iter().flatten() {
        let _ = fs::remove_file(progress_path_for(path));
    }
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
    cache: &Arc<Mutex<ArticleCache>>,
    proxy: &ProxyConfig,
    throttle_bps: f64,
    retry_count: u32,
    retry_delay: u32,
    priority: u32,
) {
    set_thread_priority(priority);
    let mut connect_attempt = 0;
    let mut nntp = loop {
        match Nntp::connect(server, proxy) {
            Ok(nntp) => break nntp,
            Err(e) => {
                if connect_attempt < retry_count {
                    connect_attempt += 1;
                    thread::sleep(Duration::from_secs(retry_delay.max(1) as u64));
                    continue;
                }
                // One failed connection should not kill the whole job; the
                // remaining workers can still finish the queue.
                if let Ok(mut list) = connect_errors.lock() {
                    list.push(e);
                }
                return;
            }
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
        let mut attempts = 0;
        let decoded = loop {
            let cached = cache
                .lock()
                .ok()
                .and_then(|mut cache| cache.get(&task.id));
            let attempt = if let Some(body) = cached {
                decode_yenc(&body).map(|(_, begin, data)| (begin, data))
            } else {
                match task.prefetched.take() {
                    Some(value) => Ok(value),
                    None => match nntp.body(&task.id) {
                        Ok(body) => {
                            if let Ok(mut cache) = cache.lock() {
                                cache.insert(task.id.clone(), body.clone());
                            }
                            decode_yenc(&body).map(|(_, begin, data)| (begin, data))
                        }
                        Err(error) => Err(error),
                    },
                }
            };
            match attempt {
                Ok(value) => break Ok(value),
                Err(error)
                    if !error.starts_with("Article not found") && attempts < retry_count =>
                {
                    attempts += 1;
                    thread::sleep(Duration::from_secs(retry_delay.max(1) as u64));
                }
                Err(error) => break Err(error),
            }
        };
        let (begin, data) = match decoded {
            Ok(value) => value,
            Err(e) => {
                // A missing article must not kill the whole job. PAR2 (or a
                // later retry) can repair the hole; record it and continue.
                if e.starts_with("Article not found") {
                    if let Ok(mut snapshot) = snap.lock() {
                        snapshot.missing += 1;
                        snapshot.message = format!("{} article(s) missing", snapshot.missing);
                    }
                } else {
                    record_error(errors, stop, e);
                    break;
                }
                continue;
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
        if let Ok(mut progress) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&task.progress)
        {
            let _ = writeln!(progress, "{}", task.id);
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

fn connect_direct(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Cannot look up {host}: {e}"))?;
    let mut last_error = String::new();
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(if last_error.is_empty() {
        format!("Cannot connect to {host}:{port}")
    } else {
        format!("Cannot connect to {host}:{port}: {last_error}")
    })
}

fn connect_via_proxy(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
    timeout: Duration,
) -> Result<TcpStream, String> {
    let proxy_host = host_only(&proxy.address);
    let proxy_port = proxy.port.max(1);
    let mut stream = connect_direct(&proxy_host, proxy_port, timeout)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    let kind = proxy.kind.trim().to_ascii_lowercase();
    if kind == "socks5" || kind == "socks" {
        socks5_connect(&mut stream, proxy, target_host, target_port)?;
    } else {
        http_connect(&mut stream, proxy, target_host, target_port)?;
    }
    Ok(stream)
}

fn http_connect(
    stream: &mut TcpStream,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<(), String> {
    let authorization = if proxy.auth {
        format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64_encode(format!("{}:{}", proxy.username, proxy.password).as_bytes())
        )
    } else {
        String::new()
    };
    let request = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n{authorization}\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("Proxy request failed: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("Proxy request failed: {error}"))?;
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    while response.len() < 16 * 1024 {
        stream
            .read_exact(&mut byte)
            .map_err(|error| format!("Proxy response failed: {error}"))?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&response);
    let status = text.lines().next().unwrap_or_default();
    let ok = status
        .split_whitespace()
        .nth(1)
        .map(|code| code == "200")
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("Proxy CONNECT failed: {status}"))
    }
}

fn socks5_connect(
    stream: &mut TcpStream,
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> Result<(), String> {
    let methods: &[u8] = if proxy.auth { &[0x00, 0x02] } else { &[0x00] };
    let mut greeting = vec![0x05, methods.len() as u8];
    greeting.extend_from_slice(methods);
    stream
        .write_all(&greeting)
        .map_err(|error| format!("SOCKS5 greeting failed: {error}"))?;
    let mut reply = [0u8; 2];
    stream
        .read_exact(&mut reply)
        .map_err(|error| format!("SOCKS5 greeting failed: {error}"))?;
    if reply[0] != 0x05 || reply[1] == 0xff {
        return Err("SOCKS5 proxy rejected the authentication methods".into());
    }
    if reply[1] == 0x02 {
        if !proxy.auth {
            return Err("SOCKS5 proxy requires authentication".into());
        }
        let user = proxy.username.as_bytes();
        let password = proxy.password.as_bytes();
        if user.len() > 255 || password.len() > 255 {
            return Err("SOCKS5 proxy credentials are too long".into());
        }
        let mut auth = vec![0x01, user.len() as u8];
        auth.extend_from_slice(user);
        auth.push(password.len() as u8);
        auth.extend_from_slice(password);
        stream
            .write_all(&auth)
            .map_err(|error| format!("SOCKS5 authentication failed: {error}"))?;
        let mut auth_reply = [0u8; 2];
        stream
            .read_exact(&mut auth_reply)
            .map_err(|error| format!("SOCKS5 authentication failed: {error}"))?;
        if auth_reply[0] != 0x01 || auth_reply[1] != 0x00 {
            return Err("SOCKS5 authentication rejected".into());
        }
    }
    let host = target_host.as_bytes();
    if host.len() > 255 {
        return Err("SOCKS5 target host name is too long".into());
    }
    let mut request = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
    request.extend_from_slice(host);
    request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&request)
        .map_err(|error| format!("SOCKS5 connect failed: {error}"))?;
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|error| format!("SOCKS5 connect failed: {error}"))?;
    if header[0] != 0x05 || header[1] != 0x00 {
        return Err(format!("SOCKS5 connect rejected with code {}", header[1]));
    }
    let address_length = match header[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut length = [0u8; 1];
            stream
                .read_exact(&mut length)
                .map_err(|error| format!("SOCKS5 connect failed: {error}"))?;
            length[0] as usize
        }
        other => return Err(format!("SOCKS5 returned unknown address type {other}")),
    };
    let mut discard = vec![0u8; address_length + 2];
    stream
        .read_exact(&mut discard)
        .map_err(|error| format!("SOCKS5 connect failed: {error}"))?;
    Ok(())
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

impl Nntp {
    fn connect(server: &Server, proxy: &ProxyConfig) -> Result<Self, String> {
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
        let stream = if proxy.is_enabled() {
            connect_via_proxy(proxy, &host, port, timeout)?
        } else {
            connect_direct(&host, port, timeout)?
        };
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

#[cfg(target_os = "windows")]
fn set_thread_priority(level: u32) {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(thread: isize, priority: i32) -> i32;
    }
    let priority = match level {
        0 => -15, // idle
        1 => -2,  // lowest
        2 => -1,  // below normal
        4 => 1,   // above normal
        5 => 2,   // highest
        _ => 0,   // normal (original default 3)
    };
    unsafe {
        SetThreadPriority(GetCurrentThread(), priority);
    }
}

#[cfg(not(target_os = "windows"))]
fn set_thread_priority(_level: u32) {}

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

fn yenc_size(body: &[u8]) -> Option<u64> {
    for line in body.split(|byte| *byte == b'\n') {
        if !line.starts_with(b"=ybegin") {
            continue;
        }
        let text = String::from_utf8_lossy(line);
        for token in text.split_whitespace() {
            if let Some(value) = token.strip_prefix("size=") {
                return value.parse::<u64>().ok();
            }
        }
        return None;
    }
    None
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
            // The encoder adds the 42 offset first and only then the 64 escape
            // offset, so an escaped byte needs both subtracted: 227 encodes to
            // 0x0D, is sent as "=M", and must decode back to 0xE3.
            out.push(encoded[i + 1].wrapping_sub(64).wrapping_sub(42));
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
        // yEnc adds the 42 offset to every byte, and then another 64 prefix plus
        // '=' when that result would be NUL, LF, CR or '='. So 19 -> '=' -> "=}",
        // 214 -> 0x00 -> "=@", 227 -> 0x0D -> "=M", 224 -> 0x0A -> "=J", and
        // 0 -> '*'.
        let body = b"=ybegin line=128 size=5 name=test.bin\n=}=@=M=J*\n=yend size=5\n";
        let (name, begin, data) = decode_yenc(body).unwrap();
        assert_eq!(name, "test.bin");
        assert_eq!(begin, None);
        assert_eq!(data, vec![19, 214, 227, 224, 0]);
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

    fn yenc_body(data: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "=ybegin line=128 size={} name=mock.bin\n",
            data.len()
        )
        .into_bytes();
        for byte in data {
            let encoded = byte.wrapping_add(42);
            if matches!(encoded, 0 | 10 | 13 | 61) {
                out.push(b'=');
                out.push(encoded.wrapping_add(64));
            } else {
                out.push(encoded);
            }
        }
        out.extend_from_slice(b"\n=yend size=10\n");
        out
    }

    fn mock_server(articles: Vec<(String, Vec<u8>)>) -> u16 {
        mock_server_counted(articles).0
    }

    fn mock_server_counted(
        articles: Vec<(String, Vec<u8>)>,
    ) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = counter.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let Ok(mut reader) = stream.try_clone().map(BufReader::new) else {
                    continue;
                };
                let _ = stream.write_all(b"200 mock ready\r\n");
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let line = line.trim();
                    if line.starts_with("AUTHINFO") {
                        let _ = stream.write_all(b"281 ok\r\n");
                    } else if line == "QUIT" {
                        break;
                    } else if let Some(id) = line.strip_prefix("BODY <") {
                        count.fetch_add(1, Ordering::Relaxed);
                        let id = id.trim_end_matches('>');
                        if let Some((_, body)) = articles.iter().find(|(key, _)| key == id) {
                            let _ = stream.write_all(b"222 0 article\r\n");
                            let _ = stream.write_all(body);
                            let _ = stream.write_all(b"\r\n.\r\n");
                        } else {
                            let _ = stream.write_all(b"430 no such article\r\n");
                        }
                    }
                }
            }
        });
        (port, counter)
    }

    fn mock_job(folder: &std::path::Path, segments: Vec<Segment>, bytes: u64) -> DownloadJob {
        DownloadJob {
            id: 1,
            folder: folder.to_string_lossy().to_string(),
            skip_existing: false,
            retry_count: 0,
            retry_delay: 1,
            priority: 3,
            cache_mb: 0,
            cache_cleanup_minutes: 30,
            proxy: ProxyConfig::default(),
            files: vec![DownloadFile {
                name: "mock.bin".into(),
                bytes,
                segments,
            }],
        }
    }

    fn wait_for(active: &ActiveDownload) -> DownloadSnapshot {
        for _ in 0..300 {
            let snapshot = active.snapshot.lock().unwrap().clone();
            if !matches!(snapshot.stage, Stage::Connecting | Stage::Downloading) {
                return snapshot;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        active.snapshot.lock().unwrap().clone()
    }

    #[test]
    fn downloads_from_a_mock_nntp_server() {
        let root = std::env::temp_dir().join(format!("newsqueue-mock-dl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data = b"ABCDEFGHIJ";
        let port = mock_server(vec![("seg1".into(), yenc_body(data))]);
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = 2;
        let job = mock_job(
            &root,
            vec![Segment {
                number: 1,
                bytes: data.len() as u64,
                id: "seg1".into(),
            }],
            data.len() as u64,
        );
        let active = spawn(job, vec![server], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), data);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn downloads_binary_articles_with_yenc_escapes() {
        // Every byte value has to survive the round trip, including the four
        // that yEnc escapes (0x00, 0x0A, 0x0D, 0x3D) and the four bytes whose
        // encoded form needs escaping (19, 214, 224, 227).
        let root = std::env::temp_dir().join(format!("newsqueue-mock-dl-bin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data: Vec<u8> = (0..=255u8).collect();
        let port = mock_server(vec![("seg1".into(), yenc_body(&data))]);
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = 2;
        let job = mock_job(
            &root,
            vec![Segment {
                number: 1,
                bytes: data.len() as u64,
                id: "seg1".into(),
            }],
            data.len() as u64,
        );
        let active = spawn(job, vec![server], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), data);
        let _ = fs::remove_dir_all(root);
    }

    fn http_proxy_stub() -> (u16, Arc<Mutex<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = Arc::new(Mutex::new(String::new()));
        let output = captured.clone();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = Vec::new();
            let mut buffer = [0u8; 1];
            while request.len() < 16 * 1024 {
                if stream.read_exact(&mut buffer).is_err() {
                    return;
                }
                request.push(buffer[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            if let Ok(mut text) = output.lock() {
                *text = String::from_utf8_lossy(&request).to_string();
            }
            let _ = stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
            let _ = stream.write_all(b"200 mock ready\r\n");
            let mut line = String::new();
            let mut reader = BufReader::new(stream);
            let _ = reader.read_line(&mut line);
        });
        (port, captured)
    }

    fn socks5_proxy_stub() -> (u16, Arc<Mutex<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured = Arc::new(Mutex::new(String::new()));
        let output = captured.clone();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut greeting = [0u8; 2];
            if stream.read_exact(&mut greeting).is_err() || greeting[0] != 5 {
                return;
            }
            let mut methods = vec![0u8; greeting[1] as usize];
            if stream.read_exact(&mut methods).is_err() {
                return;
            }
            let _ = stream.write_all(&[5, 0]);
            let mut header = [0u8; 4];
            if stream.read_exact(&mut header).is_err() || header[1] != 1 {
                return;
            }
            let host = if header[3] == 3 {
                let mut length = [0u8; 1];
                if stream.read_exact(&mut length).is_err() {
                    return;
                }
                let mut host = vec![0u8; length[0] as usize];
                if stream.read_exact(&mut host).is_err() {
                    return;
                }
                host
            } else {
                let mut host = vec![0u8; if header[3] == 1 { 4 } else { 16 }];
                if stream.read_exact(&mut host).is_err() {
                    return;
                }
                host
            };
            let mut port = [0u8; 2];
            if stream.read_exact(&mut port).is_err() {
                return;
            }
            if let Ok(mut text) = output.lock() {
                *text = format!(
                    "{}:{}",
                    String::from_utf8_lossy(&host),
                    u16::from_be_bytes(port)
                );
            }
            let _ = stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
            let _ = stream.write_all(b"200 mock ready\r\n");
            let mut line = String::new();
            let mut reader = BufReader::new(stream);
            let _ = reader.read_line(&mut line);
        });
        (port, captured)
    }

    #[test]
    fn connects_through_an_http_proxy_with_basic_auth() {
        let (port, captured) = http_proxy_stub();
        let mut server = Server::default();
        server.address = "news.invalid".into();
        server.port = 119;
        server.ssl = false;
        server.auth = false;
        let proxy = ProxyConfig {
            enabled: true,
            kind: "http".into(),
            address: "127.0.0.1".into(),
            port,
            auth: true,
            username: "user".into(),
            password: "pass".into(),
        };
        let nntp = Nntp::connect(&server, &proxy).unwrap();
        let request = captured.lock().unwrap().clone();
        assert!(request.contains("CONNECT news.invalid:119"), "{request}");
        assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz"), "{request}");
        drop(nntp);
    }

    #[test]
    fn connects_through_a_socks5_proxy() {
        let (port, captured) = socks5_proxy_stub();
        let mut server = Server::default();
        server.address = "news.invalid".into();
        server.port = 119;
        server.ssl = false;
        server.auth = false;
        let proxy = ProxyConfig {
            enabled: true,
            kind: "socks5".into(),
            address: "127.0.0.1".into(),
            port,
            auth: false,
            username: String::new(),
            password: String::new(),
        };
        let nntp = Nntp::connect(&server, &proxy).unwrap();
        assert_eq!(*captured.lock().unwrap(), "news.invalid:119");
        drop(nntp);
    }

    #[test]
    fn article_cache_respects_its_megabyte_limit() {
        let mut cache = ArticleCache::new(1, 30);
        cache.insert("first".into(), vec![0u8; 600 * 1024]);
        cache.insert("second".into(), vec![0u8; 600 * 1024]);
        assert!(cache.get("first").is_none());
        assert!(cache.get("second").is_some());
    }

    #[test]
    fn article_cache_avoids_duplicate_fetches() {
        let root = std::env::temp_dir().join(format!("newsqueue-mock-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data = b"ABCDEFGHIJ";
        let (port, count) = mock_server_counted(vec![("shared".into(), yenc_body(data))]);
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = 2;
        let job = DownloadJob {
            id: 2,
            folder: root.to_string_lossy().to_string(),
            skip_existing: false,
            retry_count: 0,
            retry_delay: 1,
            priority: 3,
            cache_mb: 1,
            cache_cleanup_minutes: 30,
            proxy: ProxyConfig::default(),
            files: vec![
                DownloadFile {
                    name: "one.bin".into(),
                    bytes: data.len() as u64,
                    segments: vec![Segment {
                        number: 1,
                        bytes: data.len() as u64,
                        id: "shared".into(),
                    }],
                },
                DownloadFile {
                    name: "two.bin".into(),
                    bytes: data.len() as u64,
                    segments: vec![Segment {
                        number: 1,
                        bytes: data.len() as u64,
                        id: "shared".into(),
                    }],
                },
            ],
        };
        let active = spawn(job, vec![server], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("one.bin")).unwrap(), data);
        assert_eq!(fs::read(root.join("two.bin")).unwrap(), data);
        assert_eq!(count.load(Ordering::Relaxed), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_article_marks_the_job_incomplete() {
        let root = std::env::temp_dir().join(format!("newsqueue-mock-miss-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data = b"ABCDEFGHIJ";
        let port = mock_server(vec![("seg1".into(), yenc_body(data))]);
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = 2;
        let job = mock_job(
            &root,
            vec![
                Segment {
                    number: 1,
                    bytes: data.len() as u64,
                    id: "seg1".into(),
                },
                Segment {
                    number: 2,
                    bytes: data.len() as u64,
                    id: "missing".into(),
                },
            ],
            data.len() as u64,
        );
        let active = spawn(job, vec![server], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Incomplete, "{}", snapshot.message);
        assert!(snapshot.missing >= 1);
        let _ = fs::remove_dir_all(root);
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
        retry_count: settings.retry_count,
        retry_delay: settings.retry_delay,
        priority: settings.download_priority,
        cache_mb: settings.article_cache_mb,
        cache_cleanup_minutes: settings.cache_cleanup_minutes,
        proxy: ProxyConfig::from_settings(&settings),
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
    let active = spawn(job, vec![server], 0.0);
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
            Stage::Incomplete => {
                println!(
                    "Download incomplete ({} missing articles): {}",
                    snapshot.missing,
                    folder.display()
                );
                return Err(snapshot.message);
            }
            Stage::Error => return Err(snapshot.message),
            Stage::Stopped => return Err("Download stopped".into()),
            _ => {}
        }
    }
}
