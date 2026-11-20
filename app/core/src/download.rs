//! Real NNTP downloading: connect, authenticate, fetch articles, undo yEnc.
//!
//! The UI runs one job at a time, but a job uses the server's configured
//! connection count so several articles are fetched in parallel. It reports
//! progress through a small shared snapshot so the UI thread never blocks
//! on the network.

mod integrity;

const MAX_ARTICLE_BYTES: usize = 64 * 1024 * 1024;

use crate::container;
use crate::model::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    /// Set only after all ranges and advertised checksums verify on disk.
    pub file_verified: Vec<bool>,
    /// Where each file is being written, once its name is known.
    pub paths: Vec<Option<PathBuf>>,
    /// Decoded size of each file from its yEnc header (0 = not known yet).
    pub sizes: Vec<u64>,
    /// Bytes downloaded from each server of the job, in the order given.
    pub server_bytes: Vec<u64>,
    /// Number of NNTP connections this job uses.
    pub workers: usize,
    pub speed_bps: f64,
    pub message: String,
    pub current_file: String,
    /// Articles the server reported as unavailable (430).
    pub missing: usize,
}

pub struct ActiveDownload {
    pub worker: Option<thread::JoinHandle<()>>,
    pub throttle_bps: Arc<AtomicU64>,
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
    /// Not part of this round (a held PAR2 volume, or already downloaded).
    pub skip: bool,
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
    /// STAT every article before downloading (`precheck_articles`).
    pub precheck: bool,
    /// Most missing articles the PAR2 set can still make up for; `None`
    /// when the collection has no PAR2 data to judge by.
    pub max_missing: Option<usize>,
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
        precheck: false,
        max_missing: None,
        files: c
            .files
            .iter()
            .map(|f| DownloadFile {
                skip: f.held || f.done,
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
    /// Indexes of servers that already answered 430 for this article.
    tried: Vec<usize>,
    transport_error: Option<String>,
    file_index: usize,
    path: PathBuf,
    offset: u64,
    id: String,
    progress: PathBuf,
    expected_size: Option<u64>,
    multipart: bool,

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
        file_verified: vec![false; job.files.len()],
        paths: vec![None; job.files.len()],
        sizes: vec![0; job.files.len()],
        server_bytes: vec![0; servers.len()],
        workers: planned_workers,
        speed_bps: 0.0,
        message: "Connecting".into(),
        current_file: String::new(),
        missing: 0,
    }));
    let throttle = Arc::new(AtomicU64::new(throttle_bps.to_bits()));
    let throttle2 = throttle.clone();
    let pause = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let snap = snapshot.clone();
    let pause2 = pause.clone();
    let stop2 = stop.clone();
    let worker = thread::spawn(move || {
        let result = run_job(&job, &servers, &throttle2, &pause2, &stop2, &snap);
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
        worker: Some(worker),
        throttle_bps: throttle,
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
    throttle_bps: &Arc<AtomicU64>,
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
    if job.precheck {
        precheck(job, servers, snap, stop, pause)?;
    }
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
    let _ = server;
    let mut lookups: Vec<Option<Nntp>> = servers.iter().map(|_| None).collect();
    let mut tasks: VecDeque<Task> = VecDeque::new();
    let mut paths: Vec<Option<PathBuf>> = vec![None; job.files.len()];
    let mut skipped = vec![false; job.files.len()];
    let mut expected_sizes: Vec<Option<u64>> = vec![None; job.files.len()];
    let mut used_names: HashSet<String> = HashSet::new();
    let mut done_bytes: u64 = 0;
    let lookup_started = Instant::now();
    for f in job.files.iter().filter(|f| f.skip) { reserve_name(&safe_file_name(&f.name, "skipped.bin"), &mut used_names); }

    for (index, file) in job.files.iter().enumerate() {
        while pause.load(Ordering::Relaxed) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if stop.load(Ordering::Relaxed) {
            quit_all(&mut lookups);
            snapshot_set(snap, |s| s.stage = Stage::Stopped);
            return Ok(());
        }
        if file.skip {
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
        let mut segments = file.segments.clone();
        if segments.is_empty() {
            continue;
        }
        segments.sort_by_key(|s| s.number);
        let fallback = format!("file{:02}.bin", index + 1);
        let base = safe_file_name(&file.name, &fallback);
        let cached = cache.lock().ok().and_then(|mut cache| cache.get(&segments[0].id));
        let mut lookup_server = None;
        let body = match cached {
            Some(body) => Some(body),
            None => match lookup_body(
                servers,
                &job.proxy,
                &mut lookups,
                &segments[0].id,
                job.retry_count,
                job.retry_delay,
                stop,
                segments.len() > 1,
            ) {
                Ok(Some((body, server_index))) => {
                    lookup_server = Some(server_index);
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(segments[0].id.clone(), body.clone());
                    }
                    Some(body)
                }
                Ok(None) => None,
                Err(e) => {
                    quit_all(&mut lookups);
                    return Err(e);
                }
            },
        };
        // A first article that no server has must not fail the whole job:
        // record it as missing (PAR2 can repair it) and fall back to the NZB
        // name. Its slot is left as a hole in the file.
        let first_missing = body.is_none();
        let (yenc_name, first_begin, first_data, first_crc) = match body.as_deref().map(decode_yenc) {
            Some(Ok(decoded)) => decoded,
            Some(Err(e)) => {
                quit_all(&mut lookups);
                return Err(e);
            }
            None => (String::new(), None, Vec::new(), None),
        };
        if let Some(server_index) = lookup_server {
            snapshot_set(snap, |s| s.server_bytes[server_index] += first_data.len() as u64);
        }
        let mut decoded_size = body.as_deref().and_then(yenc_size);
        expected_sizes[index] = decoded_size;
        let mut name = if yenc_name.is_empty() {
            base
        } else if Path::new(&file.name).extension().is_some() {
            // Obfuscated posts often put a random name in =ybegin. The NZB
            // subject still carries the intended filename and extension.
            base
        } else {
            safe_file_name(&yenc_name, &base)
        };
        // Obfuscated posts also drop the extension entirely, which hides an
        // archive from PAR2/unrar. The payload itself is the reliable source.
        if Path::new(&name).extension().is_none() {
            if let Some(renamed) = detected_name(&job.folder, &first_data, job.files.len() == 1) {
                name = renamed;
            }
        }
        // Two files of one job must never share a path, or parallel workers
        // write into each other's file. Later duplicates get " (2)", " (3)".
        let name = unique_name(&name, &mut used_names);
        let path = PathBuf::from(&job.folder).join(&name);
        paths[index] = Some(path.clone());
        if let Ok(mut s) = snap.lock() {
            s.paths[index] = Some(path.clone());
            s.sizes[index] = decoded_size.unwrap_or(0);
        }
        let progress_path = progress_path_for(&path);
        let verified_path = integrity::verified_path(&path);
        let mut records = integrity::read(&path, &progress_path, &segments);
        if !progress_path.exists() && job.skip_existing {
            records = integrity::read(&path, &verified_path, &segments);
        }
        if decoded_size.is_some_and(|size| records.values().any(|r| r.total_size != size)) { records.clear(); }
        decoded_size = decoded_size.or_else(|| records.values().next().map(|r| r.total_size));
        expected_sizes[index] = decoded_size;
        snapshot_set(snap, |s| s.sizes[index] = decoded_size.unwrap_or(0));
        let completed_ids: HashSet<String> = records.keys().cloned().collect();
        if first_missing && !completed_ids.contains(&segments[0].id) { mark_missing(snap, 1); }
        let resuming = !completed_ids.is_empty();
        if resuming {
            let resumed_bytes = records.values().map(|r| r.len).sum::<u64>();
            done_bytes = done_bytes.saturating_add(resumed_bytes);
            snapshot_set(snap, |snapshot| {
                snapshot.bytes = done_bytes;
                if let Some(progress) = snapshot.file_progress.get_mut(index) { *progress = resumed_bytes; }
            });
        }
        // Rewrite only verified records, discarding torn, stale or legacy lines.
        atomic_write(&progress_path, &integrity::encode(&records))
            .map_err(|e| format!("Cannot prepare resume metadata: {e}"))?;
        let _ = fs::remove_file(verified_path);
        let mut options = fs::OpenOptions::new();
        options.create(true).write(true);
        if !resuming {
            options.truncate(true);
        }
        if let Err(e) = options.open(&path) {
            quit_all(&mut lookups);
            return Err(format!("Cannot create file: {e}"));
        }
        let mut offset = 0u64;
        for (seg_index, segment) in segments.iter().enumerate() {
            if seg_index == 0 && first_missing {
                offset = offset.saturating_add(segment.bytes);
                continue;
            }
            if !completed_ids.contains(&segment.id) {
                let task = Task {
                    tried: Vec::new(),
                    transport_error: None,
                    file_index: index,
                    path: path.clone(),
                    offset,
                    id: segment.id.clone(),
                    progress: progress_path.clone(),
                    expected_size: decoded_size,
                    multipart: segments.len() > 1,
                };
                if seg_index == 0 {
                    if stop.load(Ordering::Relaxed) { return Err("Stopped".into()); }
                    if task.multipart && first_begin.is_none() { return Err(format!("{SIZE_MISMATCH}: multipart article has no range")); }
                    let mut files = HashMap::new();
                    let offset = first_begin.map(|b| b - 1).unwrap_or(0);
                    write_article(&mut files, &task, offset, &first_data)?;
                    integrity::append(&task, offset, &first_data, first_crc)?;
                    files.values().next().unwrap().sync_data().map_err(|e| format!("Cannot flush downloaded file: {e}"))?;
                    done_bytes = done_bytes.saturating_add(first_data.len() as u64);
                    snapshot_set(snap, |s| {
                        s.bytes = done_bytes;
                        s.file_progress[index] += first_data.len() as u64;
                        s.speed_bps = done_bytes as f64 / lookup_started.elapsed().as_secs_f64().max(0.001);
                    });
                    if segments.len() == 1 { verify_file(index, &path, file, snap)?; }
                    throttle_wait(first_data.len() as f64, throttle_bps, 1, stop, pause);
                } else { tasks.push_back(task); }
            }
            offset = offset.saturating_add(segment.bytes);
        }
    }
    quit_all(&mut lookups);

    if !tasks.is_empty() {
    // Each server gets the number of connections it is configured for,
    // interleaved so a job with few articles still uses every server.
    let mut assignment: Vec<usize> = Vec::new();
    let mut per_server: Vec<usize> = servers
        .iter()
        .map(|server| (server.connections.max(1) as usize).min(MAX_CONNECTIONS_PER_SERVER as usize))
        .collect();
    while per_server.iter().any(|count| *count > 0) {
        for (index, count) in per_server.iter_mut().enumerate() {
            if *count > 0 {
                *count -= 1;
                assignment.push(index);
            }
        }
    }
    assignment.truncate(tasks.len().max(servers.len()));
    let worker_count = assignment.len().max(1);
    let job_files = Arc::new(job.files.clone());
    snapshot_set(snap, |s| {
        s.stage = Stage::Downloading;
        s.bytes = done_bytes;
        s.workers = worker_count;
        s.message = format!("Downloading with {worker_count} connections");
    });

    let mut live = vec![0usize; servers.len()];
    for server_index in &assignment {
        live[*server_index] += 1;
    }
    let shared_tasks = Arc::new(Mutex::new(TaskQueue {
        tasks,
        in_flight: 0,
        live,
        failed: vec![None; servers.len()],
    }));
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
    for server_index in assignment {
        let server = servers[server_index].clone();
        let tasks = shared_tasks.clone();
        let errors = errors.clone();
        let connect_errors = connect_errors.clone();
        let speed = speed.clone();
        let pause = Arc::clone(pause);
        let stop = Arc::clone(stop);
        let snap = Arc::clone(snap);
        let cache = cache.clone();
        let proxy = proxy.clone();
        let throttle = throttle_bps.clone();
        let job_files = job_files.clone();
        handles.push(thread::spawn(move || {
            let context = WorkerContext {
                server_index,
                files: &job_files,
                tasks: &tasks,
                pause: &pause,
                stop: &stop,
                snap: &snap,
                speed: &speed,
                errors: &errors,
                connect_errors: &connect_errors,
                cache: &cache,
                proxy: &proxy,
                throttle_bps: &throttle,
                worker_count,
                retry_count,
                retry_delay,
                priority,
            };
            download_worker(&server, &context);
            // This connection is gone either way; articles only it could
            // still try must not wait for it.
            if let Ok(mut queue) = tasks.lock() {
                queue.live[server_index] = queue.live[server_index].saturating_sub(1);
            }
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
    let remaining = shared_tasks.lock().map(|q| q.tasks.len()).unwrap_or(0);
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

    } // Worker pool; even an empty pool must pass final integrity checks.

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

    // A size match alone cannot detect holes, overlapping parts or disk damage.
    for (index, path) in paths.iter().enumerate() {
        let Some(path) = path else { continue; };
        if skipped[index] { continue; }
        let verified = snap.lock().map(|s| s.file_verified[index]).unwrap_or(false);
        if !verified && !verify_file(index, path, &job.files[index], snap)? {
            snapshot_set(snap, |s| {
                s.stage = Stage::Incomplete;
                s.speed_bps = 0.0;
                s.message = "Downloaded ranges failed integrity verification; PAR2 repair required".into();
            });
            return Ok(());
        }
    }

    for path in paths.iter().flatten() {
        let _ = fs::remove_file(progress_path_for(path));
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

fn verify_file(index: usize, path: &Path, file: &DownloadFile, snap: &Mutex<DownloadSnapshot>) -> Result<bool, String> {
    let records = integrity::read(path, &progress_path_for(path), &file.segments);
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let expected = records.values().next().map(|r| r.total_size);
    if expected != Some(size) || !integrity::covers(&records, file.segments.len(), size) { return Ok(false); }
    atomic_write(&integrity::verified_path(path), &integrity::encode(&records))
        .map_err(|e| format!("Cannot save download verification: {e}"))?;
    snapshot_set(snap, |s| s.file_verified[index] = true);
    Ok(true)
}

/// Articles waiting for a connection, shared by every worker of a job.
struct TaskQueue {
    tasks: VecDeque<Task>,
    /// Tasks a worker has taken and not yet finished or handed back.
    in_flight: usize,
    /// Workers still alive, per server index.
    live: Vec<usize>,
    failed: Vec<Option<String>>,
}

impl TaskQueue {
    fn exhaustion_error(&self, task: &Task) -> Option<String> {
        task.transport_error.clone().or_else(|| self.failed.iter().enumerate()
            .find(|(i, error)| error.is_some() && !task.tried.contains(i))
            .and_then(|(_, error)| error.clone()))
    }

    /// True when a server that has not tried `task` still has a worker.
    fn can_retry(&self, task: &Task) -> bool {
        self.live
            .iter()
            .enumerate()
            .any(|(index, live)| *live > 0 && !task.tried.contains(&index))
    }
}

fn retry_wait(seconds: u32, stop: &AtomicBool) {
    let until = Instant::now() + Duration::from_secs(seconds.max(1) as u64);
    while !stop.load(Ordering::Relaxed) && Instant::now() < until {
        thread::sleep((until - Instant::now().min(until)).min(Duration::from_millis(25)));
    }
}

/// Pay an article's bandwidth budget in short slices so live limit changes,
/// pause and cancellation never wait behind a long low-speed sleep.
fn throttle_wait(mut bytes: f64, limit: &AtomicU64, workers: usize, stop: &AtomicBool, pause: &AtomicBool) {
    while bytes > 0.0 && !stop.load(Ordering::Relaxed) && !pause.load(Ordering::Relaxed) {
        let rate = f64::from_bits(limit.load(Ordering::Relaxed)) / workers.max(1) as f64;
        if !rate.is_finite() || rate <= 0.0 { return; }
        let started = Instant::now();
        thread::sleep(Duration::from_secs_f64((bytes / rate).min(0.025)));
        bytes -= started.elapsed().as_secs_f64() * rate;
    }
}

struct WorkerContext<'a> {
    files: &'a [DownloadFile],
    server_index: usize,
    tasks: &'a Mutex<TaskQueue>,
    pause: &'a AtomicBool,
    stop: &'a AtomicBool,
    snap: &'a Mutex<DownloadSnapshot>,
    speed: &'a Mutex<SpeedMeter>,
    errors: &'a Mutex<Vec<String>>,
    connect_errors: &'a Mutex<Vec<String>>,
    cache: &'a Arc<Mutex<ArticleCache>>,
    proxy: &'a ProxyConfig,
    throttle_bps: &'a AtomicU64,
    worker_count: usize,
    retry_count: u32,
    retry_delay: u32,
    priority: u32,
}

enum Next {
    Task(Box<Task>),
    Wait,
    Done,
}

/// Asks the servers (in order) for every article with STAT. Fails the job
/// when more are missing everywhere than `max_missing` allows.
fn precheck(job: &DownloadJob, servers: &[Server], snap: &Mutex<DownloadSnapshot>, stop: &AtomicBool, pause: &AtomicBool) -> Result<(), String> {
    let ids: Vec<&str> = job
        .files
        .iter()
        .filter(|f| !f.skip)
        .flat_map(|f| f.segments.iter().map(|s| s.id.as_str()))
        .collect();
    snapshot_set(snap, |s| s.message = format!("Pre-check: {} articles", ids.len()));
    let mut unknown: Vec<&str> = ids.clone();
    let mut inconclusive = false;
    for server in servers {
        if unknown.is_empty() {
            break;
        }
        if stop.load(Ordering::Relaxed) { return Err("Stopped".into()); }
        let Ok(mut nntp) = Nntp::connect(server, &job.proxy) else { inconclusive = true; continue };
        let mut still = Vec::new();
        for id in unknown {
            while pause.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) { thread::sleep(Duration::from_millis(25)); }
            if stop.load(Ordering::Relaxed) { return Err("Stopped".into()); }
            match nntp.stat(id) {
                Ok(true) => {}
                Ok(false) => still.push(id),
                // Some servers have no STAT: do not judge by this one.
                Err(_) => { inconclusive = true; still.push(id); },
            }
        }
        let _ = nntp.quit();
        unknown = still;
    }
    let missing = unknown.len();
    snapshot_set(snap, |s| {
        s.message = format!("Pre-check: {} of {} articles available", ids.len() - missing, ids.len())
    });
    if !inconclusive && missing > 0 && job.max_missing.map(|m| missing > m).unwrap_or(false) {
        return Err(format!(
            "Pre-check: {missing} of {} articles are missing, more than the PAR2 set can repair",
            ids.len()
        ));
    }
    Ok(())
}

fn mark_missing(snap: &Mutex<DownloadSnapshot>, count: usize) {
    if count == 0 {
        return;
    }
    if let Ok(mut snapshot) = snap.lock() {
        snapshot.missing += count;
        snapshot.message = format!("{} article(s) missing", snapshot.missing);
    }
}

/// Takes the next article this worker's server has not tried yet. Articles
/// that no living server can still try are counted as missing.
fn next_task(ctx: &WorkerContext) -> Next {
    let Ok(mut queue) = ctx.tasks.lock() else {
        return Next::Done;
    };
    let me = ctx.server_index;
    if let Some(position) = queue.tasks.iter().position(|task| !task.tried.contains(&me)) {
        if let Some(task) = queue.tasks.remove(position) {
            queue.in_flight += 1;
            return Next::Task(Box::new(task));
        }
    }
    let mut missing = 0;
    let mut transport_error = None;
    let pending: Vec<Task> = queue.tasks.drain(..).collect();
    for task in pending {
        if queue.can_retry(&task) { queue.tasks.push_back(task); }
        else if let Some(error) = queue.exhaustion_error(&task) { transport_error = Some(error); }
        else { missing += 1; }
    }
    let done = queue.tasks.is_empty() && queue.in_flight == 0;
    drop(queue);
    mark_missing(ctx.snap, missing);
    if let Some(error) = transport_error { record_error(ctx.errors, ctx.stop, error); }
    if done {
        Next::Done
    } else {
        Next::Wait
    }
}

/// Releases a taken task. `retry` hands a 430 article back for another server.
fn finish_task(ctx: &WorkerContext, retry: Option<Task>) {
    let mut missing = 0;
    let mut transport_error = None;
    if let Ok(mut queue) = ctx.tasks.lock() {
        queue.in_flight = queue.in_flight.saturating_sub(1);
        if let Some(task) = retry {
            if queue.can_retry(&task) {
                queue.tasks.push_back(task);
            } else if let Some(error) = queue.exhaustion_error(&task) { transport_error = Some(error); }
            else { missing = 1; }
        }
    }
    mark_missing(ctx.snap, missing);
    if let Some(error) = transport_error { record_error(ctx.errors, ctx.stop, error); }
}

fn download_worker(server: &Server, ctx: &WorkerContext) {
    set_thread_priority(ctx.priority);
    let mut connect_attempt = 0;
    let mut nntp = loop {
        match Nntp::connect(server, ctx.proxy) {
            Ok(nntp) => break Some(nntp),
            Err(e) => {
                if connect_attempt < ctx.retry_count && !ctx.stop.load(Ordering::Relaxed) {
                    connect_attempt += 1;
                    retry_wait(ctx.retry_delay, ctx.stop);
                    continue;
                }
                // One failed connection should not kill the whole job; the
                // remaining workers can still finish the queue.
                if let Ok(mut queue) = ctx.tasks.lock() { queue.failed[ctx.server_index] = Some(e.clone()); }
                if let Ok(mut list) = ctx.connect_errors.lock() { list.push(e); }
                return;
            }
        }
    };
    let mut files: HashMap<usize, fs::File> = HashMap::new();
    loop {
        if ctx.stop.load(Ordering::Relaxed) {
            break;
        }
        while ctx.pause.load(Ordering::Relaxed) {
            if ctx.stop.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if ctx.stop.load(Ordering::Relaxed) {
            break;
        }
        let mut task = match next_task(ctx) {
            Next::Task(task) => *task,
            Next::Wait => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Next::Done => break,
        };
        let mut attempts = 0;
        let decoded = loop {
            let cached = ctx
                .cache
                .lock()
                .ok()
                .and_then(|mut cache| cache.get(&task.id));
            let attempt = if let Some(body) = cached {
                decode_for_task(&body, &task)

            } else {
                // A connection that failed is dropped and replaced before the
                // article is asked for again, so retries never reuse a dead
                // socket.
                let connection = match nntp.take() {
                    Some(connection) => Ok(connection),
                    None => Nntp::connect(server, ctx.proxy),
                };
                match connection {
                    Ok(mut connection) => match connection.body(&task.id) {
                        Ok(body) => {
                            nntp = Some(connection);
                            let decoded = decode_for_task(&body, &task);
                            // Only a body that decoded cleanly is cached, so a
                            // corrupt copy is never served to another server's
                            // retry.
                            if decoded.is_ok() {
                                if let Ok(mut cache) = ctx.cache.lock() {
                                    cache.insert(task.id.clone(), body);
                                }
                            }
                            decoded
                        }
                        Err(error) => {
                            if unavailable(&error) {
                                nntp = Some(connection);
                            }
                            Err(error)
                        }
                    },
                    Err(error) => Err(error),
                }
            };
            match attempt {
                Ok(value) => break Ok(value),
                Err(error)
                    if !unavailable(&error)
                        && attempts < ctx.retry_count
                        && !ctx.stop.load(Ordering::Relaxed) =>
                {
                    attempts += 1;
                    retry_wait(ctx.retry_delay, ctx.stop);
                }
                Err(error) => break Err(error),
            }
        };
        let (begin, data, file_crc, total_size) = match decoded {
            Ok(value) => value,
            Err(e) => {
                if unavailable(&e) {
                    // Another server may still have it (or a good copy). If none does, it is
                    // counted missing and PAR2 can repair the hole.
                    task.tried.push(ctx.server_index);
                    finish_task(ctx, Some(task));
                    continue;
                }
                task.tried.push(ctx.server_index);
                task.transport_error = Some(e);
                finish_task(ctx, Some(task));
                continue;
            }
        };
        if ctx.stop.load(Ordering::Relaxed) { finish_task(ctx, None); break; }
        task.expected_size = Some(total_size);
        let offset = begin
            .map(|value| value.saturating_sub(1))
            .unwrap_or(task.offset);
        let write_result = write_article(&mut files, &task, offset, &data);
        if let Err(e) = write_result {
            finish_task(ctx, None);
            record_error(ctx.errors, ctx.stop, e);
            break;
        }
        if let Err(e) = integrity::append(&task, offset, &data, file_crc) {
            finish_task(ctx, None);
            record_error(ctx.errors, ctx.stop, e);
            break;
        }
        finish_task(ctx, None);
        let chunk = data.len() as u64;

        let mut measured = None;
        if let Ok(mut meter) = ctx.speed.lock() {
            meter.bytes = meter.bytes.saturating_add(chunk);
            let elapsed = meter.window.elapsed();
            if elapsed >= Duration::from_millis(500) {
                measured = Some(meter.bytes as f64 / elapsed.as_secs_f64());
                meter.bytes = 0;
                meter.window = Instant::now();
            }
        }
        if let Ok(mut s) = ctx.snap.lock() {
            s.bytes = s.bytes.saturating_add(chunk);
            if let Some(b) = s.server_bytes.get_mut(ctx.server_index) {
                *b = b.saturating_add(chunk);
            }
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

        let ready = ctx.snap.lock().map(|s| {
            task.expected_size.is_some_and(|size| s.file_progress[task.file_index] >= size)
        }).unwrap_or(false);
        if ready {
            if let Some(file) = files.get(&task.file_index) {
                if let Err(e) = file.sync_data() { record_error(ctx.errors, ctx.stop, format!("Cannot flush downloaded file: {e}")); break; }
            }
            if let Err(e) = verify_file(task.file_index, &task.path, &ctx.files[task.file_index], ctx.snap) {
                record_error(ctx.errors, ctx.stop, e); break;
            }
        }

        throttle_wait(chunk as f64, ctx.throttle_bps, ctx.worker_count, ctx.stop, ctx.pause);

    }
    if let Some(mut nntp) = nntp {
        let _ = nntp.quit();
    }
    for file in files.values_mut() {
        if let Err(e) = file.sync_data() { record_error(ctx.errors, ctx.stop, format!("Cannot flush downloaded file: {e}")); }
    }
}

/// Fetches an article during the name lookup pass, trying each server in
/// order and reconnecting after a failed connection. Returns `Ok(None)` when
/// at least one server reported the article missing and none had it.
fn lookup_body(
    servers: &[Server],
    proxy: &ProxyConfig,
    connections: &mut [Option<Nntp>],
    id: &str,
    retry_count: u32,
    retry_delay: u32,
    stop: &AtomicBool,
    multipart: bool,
) -> Result<Option<(Vec<u8>, usize)>, String> {
    let mut not_found = false;
    let mut last_error = None;
    for (index, server) in servers.iter().enumerate() {
        let mut attempts = 0;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("Stopped".into());
            }
            let connection = match connections[index].take() {
                Some(connection) => Ok(connection),
                None => Nntp::connect(server, proxy),
            };
            let result = connection.and_then(|mut connection| {
                let body = connection.body(id);
                let reusable = match &body {
                    Ok(_) => true,
                    Err(error) => unavailable(error),
                };
                if reusable {
                    connections[index] = Some(connection);
                }
                // A body whose checksum or size fails counts like a 430 so
                // the next server is asked for a good copy.
                body.and_then(|body| {
                    let (_, begin, _, _) = decode_yenc(&body)?;
                    if multipart && begin.is_none() { return Err(format!("{SIZE_MISMATCH}: multipart article has no range")); }
                    Ok(body)
                })
            });
            match result {
                Ok(body) => return Ok(Some((body, index))),
                Err(error) if unavailable(&error) => {
                    not_found = true;
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempts >= retry_count {
                        break;
                    }
                    attempts += 1;
                    retry_wait(retry_delay, stop);
                }
            }
        }
    }
    match (not_found, last_error) {
        (_, Some(error)) => Err(error),
        (true, None) => Ok(None),
        (false, None) => Err("No enabled NNTP server configured".into()),
    }
}

fn quit_all(connections: &mut [Option<Nntp>]) {
    for connection in connections.iter_mut() {
        if let Some(mut connection) = connection.take() {
            let _ = connection.quit();
        }
    }
}

fn write_article(
    files: &mut HashMap<usize, fs::File>,
    task: &Task,
    offset: u64,
    data: &[u8],
) -> Result<(), String> {
    if !files.contains_key(&task.file_index) {
        // Keep at most one output handle per worker, independent of job size.
        files.clear();
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
        let n = (&mut self.reader).take(65537)
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("Cannot read from server: {e}"))?;
        if n == 0 { return Err("Server closed the connection".into()); }
        if n > 65536 { return Err("Server response line too large".into()); }
        while matches!(line.last(), Some(&b'\n') | Some(&b'\r')) {
            line.pop();
        }
        Ok(String::from_utf8_lossy(&line).to_string())
    }

    fn body(&mut self, id: &str) -> Result<Vec<u8>, String> {
        validate_message_id(id)?;
        let response = self.command(&format!("BODY <{id}>"))?;
        if response.starts_with("430") {
            return Err(format!("{ARTICLE_NOT_FOUND}: {id}"));
        }
        if !(response.starts_with("222") || response.starts_with("220")) {
            return Err(format!("Server refused article: {}", reply_code(&response)));
        }
        let mut out = Vec::new();
        loop {
            let mut line = Vec::new();
            let n = (&mut self.reader).take((MAX_ARTICLE_BYTES - out.len() + 1) as u64)
                .read_until(b'\n', &mut line)
                .map_err(|e| format!("Cannot read article: {e}"))?;
            if n == 0 { return Err("Server closed the connection during an article".into()); }
            if out.len() + n > MAX_ARTICLE_BYTES { return Err("Article exceeds the 64 MiB safety limit".into()); }
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

    /// True when the server has the article (`223`), false on `430`/`423`.
    fn stat(&mut self, id: &str) -> Result<bool, String> {
        validate_message_id(id)?;
        let response = self.command(&format!("STAT <{id}>"))?;
        match reply_code(&response).as_str() {
            "223" => Ok(true),
            "430" | "423" => Ok(false),
            code => Err(format!("Server refused STAT: {code}")),
        }
    }

    fn quit(&mut self) -> Result<(), String> {
        let _ = self.send("QUIT");
        Ok(())
    }
}

fn validate_message_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 998 || id.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>')) {
        Err("Invalid article message ID".into())
    } else { Ok(()) }
}

fn reply_code(reply: &str) -> String {
    reply.chars().take(3).collect()
}

fn server_timeout() -> u64 {
    // Tests use a short timeout so a stalled mock server fails fast.
    if cfg!(test) {
        return 2;
    }
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

/// Returns `name`, or `stem (n).ext` when an earlier file of the job already
/// took it. Windows paths are case-insensitive, so the check is too.
fn reserve_name(name: &str, used: &mut HashSet<String>) -> bool {
    let keys = [name.to_lowercase(), format!("{}.nqpart", name.to_lowercase()), format!("{}.nqverified", name.to_lowercase())];
    if keys.iter().any(|key| used.contains(key)) { return false; }
    used.extend(keys);
    true
}

fn unique_name(name: &str, used: &mut HashSet<String>) -> String {
    if reserve_name(name, used) {
        return name.to_string();
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let mut counter = 2;
    loop {
        let candidate = format!("{stem} ({counter}){extension}");
        if reserve_name(&candidate, used) {
            return candidate;
        }
        counter += 1;
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

/// Names an obfuscated download from the payload instead of the post.
///
/// RAR volumes carry their own volume number, so a whole set can be named
/// `<collection>.partNNN.rar` and UnRAR can chain the volumes. Other containers
/// only get an extension when the collection is a single file, because several
/// unrecognised volumes sharing one name would overwrite each other.
fn detected_name(folder: &str, first_data: &[u8], single_file: bool) -> Option<String> {
    let container = container::sniff(first_data)?;
    let base = collection_base(folder);
    match container {
        container::Container::Rar5 {
            volume: Some(volume),
        } => Some(container::rar_volume_name(&base, volume)),
        _ if single_file => container::single_name(&base, container),
        _ => None,
    }
}

/// The collection name, used as the base name for detected containers.
fn collection_base(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "download".to_string())
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

fn yenc_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split(" name=").next().unwrap_or(line).split_whitespace()
        .find_map(|token| token.strip_prefix(key))
}

/// Returns the yEnc name, the 1-based begin offset from `=ypart` when the
/// article carries one, and the decoded bytes.
fn decode_yenc(body: &[u8]) -> Result<(String, Option<u64>, Vec<u8>, Option<u32>), String> {
    let mut name = String::new();
    let mut begin = None;
    let mut started = false;
    let mut total = None;
    let mut multipart = false;
    let mut part_end = None;
    let mut yend = None;
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
                let header = String::from_utf8_lossy(line);
                total = Some(yenc_field(&header, "size=").and_then(|v| v.parse::<u64>().ok())
                    .ok_or_else(|| format!("{SIZE_MISMATCH}: invalid or missing file size"))?);
                multipart = yenc_field(&header, "part=").is_some();
                started = true;
            }
            continue;
        }
        if line.starts_with(b"=yend") {
            yend = Some(String::from_utf8_lossy(line).to_string());
            break;
        }
        if line.starts_with(b"=ypart") {
            if begin.is_some() || !encoded.is_empty() { return Err(format!("{INVALID_ARTICLE}: misplaced part header")); }
            let header = String::from_utf8_lossy(line);
            begin = Some(yenc_field(&header, "begin=").and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| format!("{INVALID_ARTICLE}: invalid part begin"))?);
            part_end = Some(yenc_field(&header, "end=").and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| format!("{INVALID_ARTICLE}: invalid part end"))?);
            continue;
        }
        encoded.extend_from_slice(line);
    }
    if !started {
        return Err(format!("{INVALID_ARTICLE}: no yEnc body"));
    }
    let mut out = Vec::with_capacity(encoded.len());
    let mut i = 0;
    while i < encoded.len() {
        let c = encoded[i];
        if c == b'=' {
            if i + 1 >= encoded.len() {
                return Err(format!("{INVALID_ARTICLE}: broken escape sequence"));
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
    let trailer = yend.ok_or_else(|| format!("{INVALID_ARTICLE}: missing yEnc trailer"))?;
    let expected = yenc_field(&trailer, "size=").and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| format!("{SIZE_MISMATCH}: invalid or missing trailer size"))?;
    if expected != out.len() as u64 { return Err(format!("{SIZE_MISMATCH}: expected {expected}, got {}", out.len())); }
    let size = total.unwrap_or(0);
    match (begin, part_end) {
        (Some(b), Some(e)) if b > 0 && e >= b && e <= size && e - b + 1 == expected => {},
        (None, None) if !multipart && expected == size => {},
        _ => return Err(format!("{SIZE_MISMATCH}: invalid part range or file size")),
    }
    // Reject malformed checksum fields instead of silently ignoring them.
    for key in ["pcrc32=", "crc32="] {
        if let Some(raw) = yenc_field(&trailer, key) {
            let crc = u32::from_str_radix(raw, 16).ok().filter(|_| !raw.is_empty() && raw.len() <= 8 && raw.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| format!("{CRC_MISMATCH}: invalid checksum"))?;
            if (key == "pcrc32=" || begin.is_none()) && crc32(&out) != crc {
                return Err(format!("{CRC_MISMATCH}: expected {crc:08x}, got {:08x}", crc32(&out)));
            }
        }
    }
    let file_crc = yenc_field(&trailer, "crc32=").and_then(|v| u32::from_str_radix(v, 16).ok());
    Ok((name, begin, out, file_crc))
}

fn decode_for_task(body: &[u8], task: &Task) -> Result<(Option<u64>, Vec<u8>, Option<u32>, u64), String> {
    let (_, begin, data, crc) = decode_yenc(body)?;
    if (task.multipart && begin.is_none())
        || task.expected_size.is_some_and(|size| yenc_size(body) != Some(size)) {
        return Err(format!("{SIZE_MISMATCH}: inconsistent multipart header"));
    }
    Ok((begin, data, crc, yenc_size(body).ok_or("Missing advertised file size")?))
}

const INVALID_ARTICLE: &str = "Invalid yEnc article";
const ARTICLE_NOT_FOUND: &str = "Article not found";
const CRC_MISMATCH: &str = "Article CRC mismatch";
const SIZE_MISMATCH: &str = "Article size mismatch";

/// True for errors where another server may still have a good copy: a 430,
/// or a body whose yEnc checksum or decoded size does not match.
fn unavailable(error: &str) -> bool {
    error.starts_with(INVALID_ARTICLE)
        || error.starts_with(ARTICLE_NOT_FOUND)
        || error.starts_with(CRC_MISMATCH)
        || error.starts_with(SIZE_MISMATCH)
}

/// CRC-32 (IEEE 802.3, the polynomial yEnc uses).
fn crc32(data: &[u8]) -> u32 { crc32fast::hash(data) }

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn throttle_changes_and_stop_interrupt_an_existing_wait() {
        for stop_wait in [false, true] {
            let limit = Arc::new(AtomicU64::new(1.0f64.to_bits()));
            let stop = Arc::new(AtomicBool::new(false));
            let (limit2, stop2) = (limit.clone(), stop.clone());
            let started = Instant::now();
            let worker = thread::spawn(move || throttle_wait(1_000_000.0, &limit2, 8, &stop2, &AtomicBool::new(false)));
            thread::sleep(Duration::from_millis(50));
            assert!(!worker.is_finished());
            if stop_wait { stop.store(true, Ordering::Relaxed); } else { limit.store(0.0f64.to_bits(), Ordering::Relaxed); }
            worker.join().unwrap();
            assert!(started.elapsed() < Duration::from_secs(2));
        }
    }

    #[test]
    fn audit_rejects_truncated_and_malformed_yenc() {
        for body in [
            "=ybegin size=1 name=a.bin\n*\n",
            "=ybegin size=1 name=a.bin\n*\n=yend size=1 crc32=garbage\n",
            "=ybegin part=1 size=1 name=a.bin\n=ypart begin=0 end=0\n*\n=yend size=1\n",
            "=ybegin part=1 size=1 name=a.bin\n=ypart begin=1 end=2\n*\n=yend size=1\n",
        ] { assert!(decode_yenc(body.as_bytes()).is_err(), "accepted {body:?}"); }
    }

    #[test]
    fn audit_all_articles_missing_is_incomplete() {
        let root = temp_root("audit-all-missing");
        let port = mock_server(vec![]);
        let job = mock_job(&root, vec![Segment { number: 1, bytes: 5, id: "absent".into() }], 5);
        let active = spawn(job, vec![local_server(port, 1)], 0.0);
        assert_eq!(wait_for(&active).stage, Stage::Incomplete);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_full_length_partial_file_is_not_skipped() {
        let root = temp_root("audit-full-partial");
        fs::write(root.join("mock.bin"), b"XXXXXFGHIJ").unwrap();
        fs::write(progress_path_for(&root.join("mock.bin")), "p2\n").unwrap();
        let port = mock_server(vec![("p1".into(), yenc_part(b"ABCDE", 10, 1)), ("p2".into(), yenc_part(b"FGHIJ", 10, 6))]);
        let mut job = two_part_job(&root); job.skip_existing = true;
        let active = spawn(job, vec![local_server(port, 2)], 0.0);
        assert_eq!(wait_for(&active).stage, Stage::Completed);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_checks_whole_file_crc_and_rejects_overlap() {
        for overlap in [false, true] {
            let root = temp_root(if overlap { "audit-overlap" } else { "audit-whole-crc" });
            let mut last = yenc_part(b"FGHIJ", 10, if overlap { 1 } else { 6 });
            if !overlap {
                last = String::from_utf8(last).unwrap().replace("=yend size=5", "=yend size=5 crc32=00000001").into_bytes();
            }
            let port = mock_server(vec![("p1".into(), yenc_part(b"ABCDE", 10, 1)), ("p2".into(), last)]);
            let active = spawn(two_part_job(&root), vec![local_server(port, 2)], 0.0);
            assert_eq!(wait_for(&active).stage, Stage::Incomplete);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn audit_disk_damage_and_missing_payload_are_redownloaded() {
        for missing in [false, true] {
            let root = temp_root(if missing { "audit-orphan-receipt" } else { "audit-disk-damage" });
            let path = root.join("mock.bin");
            if !missing { fs::write(&path, b"ABCDEXXXXX").unwrap(); }
            let records = [
                integrity::Record { id:"p1".into(), offset:0, len:5, total_size:10, crc:crc32(b"ABCDE"), file_crc:None },
                integrity::Record { id:"p2".into(), offset:5, len:5, total_size:10, crc:crc32(b"FGHIJ"), file_crc:None },
            ].into_iter().map(|r| (r.id.clone(),r)).collect();
            fs::write(progress_path_for(&path), integrity::encode(&records)).unwrap();
            let (port,count) = mock_server_counted(vec![("p1".into(), yenc_part(b"ABCDE",10,1)), ("p2".into(), yenc_part(b"FGHIJ",10,6))]);
            let active = spawn(two_part_job(&root), vec![local_server(port,2)],0.0);
            assert_eq!(wait_for(&active).stage,Stage::Completed);
            assert_eq!(fs::read(path).unwrap(),b"ABCDEFGHIJ");
            assert_eq!(count.load(Ordering::Relaxed),2);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn audit_verified_resume_survives_missing_first_article() {
        let root = temp_root("audit-resume-first-missing");
        let path = root.join("mock.bin"); fs::write(&path,b"ABCDEFGHIJ").unwrap();
        let records = [
            integrity::Record { id:"p1".into(),offset:0,len:5,total_size:10,crc:crc32(b"ABCDE"),file_crc:None },
            integrity::Record { id:"p2".into(),offset:5,len:5,total_size:10,crc:crc32(b"FGHIJ"),file_crc:None },
        ].into_iter().map(|r|(r.id.clone(),r)).collect();
        fs::write(progress_path_for(&path),integrity::encode(&records)).unwrap();
        let port=mock_server(vec![]);
        let active=spawn(two_part_job(&root),vec![local_server(port,1)],0.0);
        assert_eq!(wait_for(&active).stage,Stage::Completed);
        assert_eq!(fs::read(path).unwrap(),b"ABCDEFGHIJ");
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn audit_resumed_file_cannot_hide_missing_tail_when_first_article_is_gone() {
        let root=temp_root("audit-resume-missing-tail");let path=root.join("mock.bin");fs::write(&path,b"ABCDE").unwrap();
        let first=integrity::Record{id:"p1".into(),offset:0,len:5,total_size:15,crc:crc32(b"ABCDE"),file_crc:None};
        fs::write(progress_path_for(&path),format!("{}\n",serde_json::to_string(&first).unwrap())).unwrap();
        let port=mock_server(vec![("p2".into(),yenc_part(b"FGHIJ",15,6))]);
        let active=spawn(two_part_job(&root),vec![local_server(port,1)],0.0);
        assert_eq!(wait_for(&active).stage,Stage::Incomplete);
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn audit_first_part_context_uses_backup_and_one_pending_part_can_fail_over() {
        let root=temp_root("audit-context-failover");
        let primary=mock_server(vec![("p1".into(),yenc_body(b"ABCDEFGHIJ"))]);
        let backup=mock_server(vec![("p1".into(),yenc_part(b"ABCDE",10,1)),("p2".into(),yenc_part(b"FGHIJ",10,6))]);
        let active=spawn(two_part_job(&root),vec![local_server(primary,1),local_server(backup,1)],0.0);
        assert_eq!(wait_for(&active).stage,Stage::Completed);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(),b"ABCDEFGHIJ");
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn audit_duplicate_receipt_cannot_bypass_whole_file_crc() {
        let root=temp_root("audit-duplicate-receipt");let path=root.join("mock.bin");fs::write(&path,b"ABCDE").unwrap();
        let record=integrity::Record{id:"p1".into(),offset:0,len:5,total_size:5,crc:crc32(b"ABCDE"),file_crc:Some(1)};
        let line=serde_json::to_string(&record).unwrap();fs::write(progress_path_for(&path),format!("{line}\n{line}\n")).unwrap();
        assert!(integrity::read(&path,&progress_path_for(&path),&[Segment{number:1,bytes:5,id:"p1".into()}]).is_empty());
        let _=fs::remove_dir_all(root);
    }

    #[test]
    fn audit_metadata_names_and_message_ids_are_safe() {
        let mut used=HashSet::new();assert_eq!(unique_name("a.bin",&mut used),"a.bin");
        assert_ne!(unique_name("a.bin.nqpart",&mut used),"a.bin.nqpart");
        assert_ne!(unique_name("a.bin.nqverified",&mut used),"a.bin.nqverified");
        for bad in ["", "a\r\nQUIT", "<a>", "a b"] { assert!(validate_message_id(bad).is_err()); }
    }

    #[test]
    fn decodes_yenc_escapes() {
        // yEnc adds the 42 offset to every byte, and then another 64 prefix plus
        // '=' when that result would be NUL, LF, CR or '='. So 19 -> '=' -> "=}",
        // 214 -> 0x00 -> "=@", 227 -> 0x0D -> "=M", 224 -> 0x0A -> "=J", and
        // 0 -> '*'.
        let body = b"=ybegin line=128 size=5 name=test.bin\n=}=@=M=J*\n=yend size=5\n";
        let (name, begin, data, _) = decode_yenc(body).unwrap();
        assert_eq!(name, "test.bin");
        assert_eq!(begin, None);
        assert_eq!(data, vec![19, 214, 227, 224, 0]);
    }

    #[test]
    fn reads_the_yenc_part_offset() {
        let body = b"=ybegin part=2 total=3 line=128 size=9 name=test.bin\n=ypart begin=4 end=6\n*+,\n=yend size=3\n";
        let (_, begin, data, _) = decode_yenc(body).unwrap();
        assert_eq!(begin, Some(4));
        assert_eq!(data, vec![0, 1, 2]);
    }

    #[test]
    fn strips_a_nntp_server_url_to_its_host() {
        assert_eq!(host_only("https://news.easynews.com"), "news.easynews.com");
        assert_eq!(host_only("news.easynews.com:563"), "news.easynews.com");
    }

    fn yenc_body(data: &[u8]) -> Vec<u8> {
        yenc_body_named(data, "mock.bin")
    }

    fn yenc_body_named(data: &[u8], name: &str) -> Vec<u8> {
        let mut out = format!(
            "=ybegin line=128 size={} name={}\n",
            data.len(),
            name
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
        out.extend_from_slice(format!("\n=yend size={}\n", data.len()).as_bytes());
        out
    }

    fn mock_server(articles: Vec<(String, Vec<u8>)>) -> u16 {
        mock_server_counted(articles).0
    }

    pub(crate) fn mock_server_counted(
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
                    } else if let Some(id) = line.strip_prefix("STAT <") {
                        let id = id.trim_end_matches('>');
                        let have = articles.iter().any(|(key, _)| key == id);
                        let _ = stream.write_all(if have { b"223 0 ok\r\n" } else { b"430 no such article\r\n" });
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
            precheck: false,
            max_missing: None,
            files: vec![DownloadFile {
                skip: false,
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
    fn names_an_obfuscated_rar_volume_after_its_header() {
        // A post with no usable name or extension, carrying the first volume of
        // a RAR5 set. The app must land on `<collection>.part001.rar`.
        let root = std::env::temp_dir().join(format!("newsqueue-mock-dl-rar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut data = b"Rar!\x1a\x07\x01\x00".to_vec();
        data.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd, 16, 1, 5, 12, 3, 0, 0x0b, 1, 1]);
        data.resize(256, 0x55);
        let port = mock_server(vec![("seg1".into(), yenc_body_named(&data, "f4h2k9d0sj3lq8zr"))]);
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = 2;
        let mut job = mock_job(
            &root,
            vec![Segment {
                number: 1,
                bytes: data.len() as u64,
                id: "seg1".into(),
            }],
            data.len() as u64,
        );
        job.files[0].name = "f4h2k9d0sj3lq8zr".into();
        let collection = root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let active = spawn(job, vec![server], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        let expected = root.join(format!("{collection}.part001.rar"));
        assert!(expected.is_file(), "expected {}", expected.display());
        assert_eq!(fs::read(&expected).unwrap(), data);
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
            precheck: false,
            max_missing: None,
            files: vec![
                DownloadFile {
                    skip: false,
                    name: "one.bin".into(),
                    bytes: data.len() as u64,
                    segments: vec![Segment {
                        number: 1,
                        bytes: data.len() as u64,
                        id: "shared".into(),
                    }],
                },
                DownloadFile {
                    skip: false,
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

    /// One yEnc part of a `total`-byte file starting at 1-based `begin`.
    pub(crate) fn yenc_part(data: &[u8], total: usize, begin: usize) -> Vec<u8> {
        let mut out = format!(
            "=ybegin part=1 line=128 size={total} name=mock.bin\n=ypart begin={begin} end={}\n",
            begin + data.len() - 1
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
        out.extend_from_slice(format!("\n=yend size={}\n", data.len()).as_bytes());
        out
    }

    /// Like `mock_server`, but closes the connection without answering the
    /// `drop_on`-th BODY command (1-based, counted across connections).
    fn flaky_server(articles: Vec<(String, Vec<u8>)>, drop_on: usize) -> u16 {
        misbehaving_server(articles, drop_on, false)
    }

    /// `flaky_server`, or with `stall` the connection stays open and silent
    /// on that BODY, so only the client's read timeout can end the wait.
    fn misbehaving_server(articles: Vec<(String, Vec<u8>)>, drop_on: usize, stall: bool) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut bodies = 0;
            for _ in 0..8 {
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
                    if line == "QUIT" {
                        break;
                    } else if let Some(id) = line.strip_prefix("BODY <") {
                        bodies += 1;
                        if bodies == drop_on {
                            if stall {
                                // Hold the socket open on another thread and
                                // accept the client's next connection.
                                std::thread::spawn(move || {
                                    std::thread::sleep(Duration::from_secs(30));
                                    drop(stream);
                                });
                            } else {
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                            }
                            break;
                        }
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
        port
    }

    pub(crate) fn local_server(port: u16, connections: u32) -> Server {
        let mut server = Server::default();
        server.address = "127.0.0.1".into();
        server.port = port;
        server.ssl = false;
        server.auth = false;
        server.enabled = true;
        server.connections = connections;
        server
    }

    fn two_part_job(root: &std::path::Path) -> DownloadJob {
        mock_job(
            root,
            vec![
                Segment {
                    number: 1,
                    bytes: 5,
                    id: "p1".into(),
                },
                Segment {
                    number: 2,
                    bytes: 5,
                    id: "p2".into(),
                },
            ],
            10,
        )
    }

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("newsqueue-mock-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// `yenc_part` with a `pcrc32` trailer computed over `crc_of`.
    fn yenc_part_crc(data: &[u8], total: usize, begin: usize, crc_of: &[u8]) -> Vec<u8> {
        let body = yenc_part(data, total, begin);
        let text = String::from_utf8_lossy(&body).to_string();
        let trailer = format!("=yend size={}", data.len());
        text.replace(
            &trailer,
            &format!("{trailer} part=1 pcrc32={:08x}", crc32(crc_of)),
        )
        .into_bytes()
    }

    #[test]
    fn crc32_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn audit_crc_accepts_omitted_leading_zeroes_only() {
        let payload=(0u32..10000).find_map(|n| { let bytes=n.to_le_bytes();(crc32(&bytes)<0x1000_0000).then_some(bytes) }).unwrap();
        let part=yenc_part_crc(&payload,payload.len(),1,&payload);
        let crc=crc32(&payload);
        let shortened=String::from_utf8(part).unwrap().replace(&format!("{crc:08x}"),&format!("{crc:x}"));
        assert_eq!(decode_yenc(shortened.as_bytes()).unwrap().2,payload);
        for bad in ["+1234567","000000000","xyz",""] {
            let body=shortened.replace(&format!("pcrc32={crc:x}"),&format!("pcrc32={bad}"));
            assert!(decode_yenc(body.as_bytes()).is_err(),"accepted {bad}");
        }
    }

    #[test]
    fn decode_checks_the_part_crc() {
        let good = yenc_part_crc(b"ABCDE", 10, 1, b"ABCDE");
        assert_eq!(decode_yenc(&good).unwrap().2, b"ABCDE");
        let bad = yenc_part_crc(b"ABCDE", 10, 1, b"ABCDX");
        let error = decode_yenc(&bad).unwrap_err();
        assert!(error.starts_with(CRC_MISMATCH), "{error}");
        // A single-part post may carry only crc32= for the whole file.
        let single = b"=ybegin line=128 size=1 name=a.bin\n*\n=yend size=1 crc32=d202ef8d\n";
        assert_eq!(decode_yenc(single).unwrap().2, vec![0]);
        let wrong = b"=ybegin line=128 size=1 name=a.bin\n*\n=yend size=1 crc32=00000001\n";
        assert!(decode_yenc(wrong).is_err());
    }

    #[test]
    fn decode_checks_trailer_size_per_article() {
        let part = yenc_part(b"ABCDE", 10, 1);
        assert_eq!(decode_yenc(&part).unwrap().2, b"ABCDE");
        let short = b"=ybegin line=128 size=5 name=a.bin\nklmn\n=yend size=5\n";
        let error = decode_yenc(short).unwrap_err();
        assert!(error.starts_with(SIZE_MISMATCH), "{error}");
        assert!(unavailable(&error));
        let malformed = b"=ybegin line=128 size=1 name=a.bin\n*\n=yend size=nope\n";
        assert!(decode_yenc(malformed)
            .unwrap_err()
            .starts_with(SIZE_MISMATCH));
        // Missing size/trailer cannot establish that an article is complete.
        for body in [
            &b"=ybegin name=a.bin\n*\n=yend\n"[..],
            &b"=ybegin name=a.bin\n*\n"[..],
        ] {
            assert!(decode_yenc(body).is_err());
        }
    }

    #[test]
    fn a_size_mismatch_is_fetched_from_the_next_server() {
        let root = temp_root("size-failover");
        // Name lookup fetches p1 from servers in order, so it must encounter
        // this truncated primary copy before trying the backup.
        let bad = concat!(
            "=ybegin part=1 line=128 size=10 name=mock.bin\n",
            "=ypart begin=1 end=5\nklmn\n=yend size=5\n"
        )
        .as_bytes()
        .to_vec();
        let (primary, primary_count) = mock_server_counted(vec![
            ("p1".into(), bad),
            ("p2".into(), yenc_part(b"FGHIJ", 10, 6)),
        ]);
        let (backup, backup_count) = mock_server_counted(vec![
            ("p1".into(), yenc_part(b"ABCDE", 10, 1)),
            ("p2".into(), yenc_part(b"FGHIJ", 10, 6)),
        ]);
        let mut job = two_part_job(&root);
        job.cache_mb = 1;
        let active = spawn(
            job,
            vec![local_server(primary, 1), local_server(backup, 1)],
            0.0,
        );
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(snapshot.missing, 0);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        assert!(primary_count.load(Ordering::Relaxed) >= 1);
        assert!(backup_count.load(Ordering::Relaxed) >= 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_corrupt_article_is_fetched_from_the_next_server() {
        let root = temp_root("crc-failover");
        let primary = mock_server(vec![
            ("p1".into(), yenc_part_crc(b"ABCDE", 10, 1, b"ABCDE")),
            ("p2".into(), yenc_part_crc(b"FGHIX", 10, 6, b"FGHIJ")),
        ]);
        let backup = mock_server(vec![
            ("p1".into(), yenc_part_crc(b"ABCDE", 10, 1, b"ABCDE")),
            ("p2".into(), yenc_part_crc(b"FGHIJ", 10, 6, b"FGHIJ")),
        ]);
        let mut job = two_part_job(&root);
        job.cache_mb = 1;
        let servers = vec![local_server(primary, 1), local_server(backup, 1)];
        let active = spawn(job, servers, 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn files_with_the_same_name_get_separate_paths() {
        let root = temp_root("dupe-names");
        let port = mock_server(vec![
            ("a".into(), yenc_body(b"first")),
            ("b".into(), yenc_body(b"second")),
        ]);
        let mut job = mock_job(&root, Vec::new(), 0);
        job.files = ["a", "b"]
            .iter()
            .map(|id| DownloadFile {
                skip: false,
                name: "Same.Name.bin".into(),
                bytes: 6,
                segments: vec![Segment {
                    number: 1,
                    bytes: 6,
                    id: (*id).into(),
                }],
            })
            .collect();
        let active = spawn(job, vec![local_server(port, 2)], 0.0);
        let snapshot = wait_for(&active);
        assert_ne!(snapshot.stage, Stage::Error, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("Same.Name.bin")).unwrap(), b"first");
        assert_eq!(fs::read(root.join("Same.Name (2).bin")).unwrap(), b"second");
        let mut used = HashSet::new();
        assert_eq!(unique_name("x.rar", &mut used), "x.rar");
        assert_eq!(unique_name("X.RAR", &mut used), "X (2).RAR");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_430_on_one_server_is_fetched_from_the_next() {
        let root = temp_root("failover");
        let primary = mock_server(vec![("p1".into(), yenc_part(b"ABCDE", 10, 1))]);
        let backup = mock_server(vec![
            ("p1".into(), yenc_part(b"ABCDE", 10, 1)),
            ("p2".into(), yenc_part(b"FGHIJ", 10, 6)),
        ]);
        let servers = vec![local_server(primary, 1), local_server(backup, 1)];
        let active = spawn(two_part_job(&root), servers, 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(snapshot.missing, 0);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_first_article_marks_the_job_incomplete_not_failed() {
        let root = temp_root("first-missing");
        let port = mock_server(vec![("p2".into(), yenc_part(b"FGHIJ", 10, 6))]);
        let active = spawn(two_part_job(&root), vec![local_server(port, 1)], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Incomplete, "{}", snapshot.message);
        assert_eq!(snapshot.missing, 1);
        let written = fs::read(root.join("mock.bin")).unwrap();
        assert_eq!(&written[5..], b"FGHIJ");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_dropped_connection_is_replaced_before_the_retry() {
        let root = temp_root("reconnect");
        // BODY #1 is the lookup of p1, #2 is the worker's p2, which is dropped.
        let port = flaky_server(
            vec![
                ("p1".into(), yenc_part(b"ABCDE", 10, 1)),
                ("p2".into(), yenc_part(b"FGHIJ", 10, 6)),
            ],
            2,
        );
        let mut job = two_part_job(&root);
        job.retry_count = 1;
        let active = spawn(job, vec![local_server(port, 1)], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_stalled_read_times_out_and_is_retried_on_a_new_connection() {
        let root = temp_root("stall");
        // BODY #1 is the lookup of p1, #2 is the worker's p2: the server goes silent.
        let port = misbehaving_server(
            vec![
                ("p1".into(), yenc_part(b"ABCDE", 10, 1)),
                ("p2".into(), yenc_part(b"FGHIJ", 10, 6)),
            ],
            2,
            true,
        );
        let mut job = two_part_job(&root);
        job.retry_count = 1;
        let started = std::time::Instant::now();
        let active = spawn(job, vec![local_server(port, 1)], 0.0);
        let mut snapshot = wait_for(&active);
        while matches!(snapshot.stage, Stage::Connecting | Stage::Downloading) && started.elapsed() < Duration::from_secs(20) {
            snapshot = wait_for(&active);
        }
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"ABCDEFGHIJ");
        assert!(started.elapsed() >= Duration::from_secs(2), "the stall should cost one read timeout");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn precheck_refuses_a_job_with_too_many_missing_articles() {
        let root = temp_root("precheck");
        // p2 is gone everywhere; the PAR2 set could make up for none.
        let (port, count) = mock_server_counted(vec![("p1".into(), yenc_part(b"ABCDE", 10, 1))]);
        let mut job = two_part_job(&root);
        job.precheck = true;
        job.max_missing = Some(0);
        let active = spawn(job, vec![local_server(port, 1)], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Error, "{}", snapshot.message);
        assert!(snapshot.message.contains("Pre-check: 1 of 2"), "{}", snapshot.message);
        assert_eq!(count.load(Ordering::Relaxed), 0, "nothing is downloaded");
        let _ = fs::remove_dir_all(&root);

        // Within budget (or no PAR2 to judge by): the job runs.
        let root = temp_root("precheck-ok");
        let (port, _) = mock_server_counted(vec![("p1".into(), yenc_part(b"ABCDE", 10, 1))]);
        let mut job = two_part_job(&root);
        job.precheck = true;
        job.max_missing = Some(1);
        let snapshot = wait_for(&spawn(job, vec![local_server(port, 1)], 0.0));
        assert_eq!(snapshot.stage, Stage::Incomplete, "{}", snapshot.message);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skipped_files_are_neither_fetched_nor_written() {
        let root = temp_root("skip");
        let (port, count) = mock_server_counted(vec![("p2".into(), yenc_part(b"FGHIJ", 5, 1))]);
        let mut job = two_part_job(&root);
        let first = DownloadFile {
            skip: true,
            name: "held.vol00+01.par2".into(),
            bytes: 5,
            segments: vec![Segment { number: 1, bytes: 5, id: "p1".into() }],
        };
        job.files[0].segments.remove(0);
        job.files[0].bytes = 5;
        job.files.insert(0, first);
        let active = spawn(job, vec![local_server(port, 1)], 0.0);
        let snapshot = wait_for(&active);
        assert_eq!(snapshot.stage, Stage::Completed, "{}", snapshot.message);
        assert!(!root.join("held.vol00+01.par2").exists());
        assert_eq!(fs::read(root.join("mock.bin")).unwrap(), b"FGHIJ");
        assert_eq!(count.load(Ordering::Relaxed), 1, "only p2 is fetched");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_article_marks_the_job_incomplete() {
        let root = std::env::temp_dir().join(format!("newsqueue-mock-miss-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data = b"ABCDEFGHIJ";
        let port = mock_server(vec![("seg1".into(), yenc_part(data, data.len() * 2, 1))]);
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
        precheck: false,
        max_missing: None,
        files: parsed
            .files
            .iter()
            .map(|f| DownloadFile {
                skip: false,
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
