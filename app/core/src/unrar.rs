//! RAR extraction for completed collections.
//!
//! The original loads `unrar.dll`. This build is 64-bit and the bundled
//! AltBinz DLL is 32-bit, so extraction runs the standalone `UnRAR.exe`
//! (vendored in `tools/`) in a worker thread. The UI polls a channel, just
//! like the NNTP downloader.

use crate::container::{self, Container};
use crate::model::{app_base, sanitize_folder, Collection, Settings};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

pub struct ActiveUnrar {
    pub collection_id: u64,
    pub rx: Receiver<Result<String, UnrarError>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnrarError {
    PasswordRequired,
    Failed(String),
}

impl std::fmt::Display for UnrarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PasswordRequired => formatter.write_str("Unrar password required"),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for UnrarError {}

#[derive(Clone, Debug)]
pub struct UnrarOptions {
    pub overwrite: bool,
    pub rename_existing: bool,
    pub strip_root: bool,
    pub delete_archives: bool,
    pub delete_empty_folder: bool,
    pub auto_rename: bool,
    pub collection_name: String,
    pub password: String,
    pub move_patterns: Vec<String>,
    pub delete_patterns: Vec<String>,
    pub case_insensitive: bool,
    pub expected_files: Vec<String>,
}

impl UnrarOptions {
    pub fn from_collection(settings: &Settings, collection: &Collection) -> Self {
        Self {
            overwrite: settings.overwrite_existing,
            rename_existing: settings.rename_existing,
            strip_root: settings.strip_root,
            delete_archives: settings.delete_archives || collection.delete_archives,
            delete_empty_folder: settings.delete_empty_folder,
            auto_rename: settings.auto_rename_to_collection,
            collection_name: collection.subject.clone(),
            password: collection.password.clone(),
            move_patterns: split_patterns(&settings.move_files_to_unrar),
            delete_patterns: split_patterns(&settings.delete_files_after_unrar),
            case_insensitive: settings.case_insensitive_filters,
            expected_files: collection
                .files
                .iter()
                .map(|file| file.name.clone())
                .collect(),
        }
    }
}

/// Target directory for a collection's archive contents.
pub fn target_dir(settings: &Settings, collection: &Collection) -> PathBuf {
    let mut base = if !collection.unrar_folder.trim().is_empty() {
        PathBuf::from(&collection.unrar_folder)
    } else if settings.unrar_to_different {
        PathBuf::from(&settings.unrar_dir)
    } else {
        PathBuf::from(&collection.folder)
    };
    if settings.create_dir_archive_name {
        let expected: Vec<String> = collection
            .files
            .iter()
            .map(|file| file.name.clone())
            .collect();
        if let Some(archive) = find_archive(Path::new(&collection.folder), &expected) {
            if let Some(stem) = archive.file_stem() {
                base = base.join(sanitize_folder(&stem.to_string_lossy()));
            }
        }
    }
    if settings.create_dir_group_name && !collection.group.trim().is_empty() {
        base = base.join(sanitize_folder(&collection.group));
    }
    if settings.create_dir_nzb_name {
        base = base.join(sanitize_folder(&collection.subject));
    }
    base
}

pub fn spawn(collection_id: u64, source: PathBuf, dest: PathBuf, options: UnrarOptions) -> ActiveUnrar {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = extract(&source, &dest, &options);
        let _ = tx.send(result);
    });
    ActiveUnrar { collection_id, rx }
}

pub fn poll(active: &ActiveUnrar) -> Option<Result<String, UnrarError>> {
    match active.rx.try_recv() {
        Ok(value) => Some(value),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(UnrarError::Failed(
            "Unrar worker stopped unexpectedly".into(),
        ))),
    }
}

/// A RAR set being extracted while it downloads ("direct unpack").
pub struct DirectUnrar {
    pub collection_id: u64,
    /// Volumes whose articles are all written.
    pub ready: Arc<Mutex<Vec<PathBuf>>>,
    /// The download is over: any volume that exists now is as good as it gets.
    pub finished: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<Result<String, UnrarError>>,
}

impl DirectUnrar {
    pub fn poll(&self) -> Option<Result<String, UnrarError>> {
        match self.rx.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(UnrarError::Failed("Direct unpack worker stopped".into()))),
        }
    }
}

impl Drop for DirectUnrar {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Whether `name` is the volume a RAR set is opened from.
pub fn is_first_rar_volume(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".rar") else {
        return false;
    };
    match stem.rfind(".part") {
        Some(at) => {
            let digits = &stem[at + 5..];
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && digits.trim_start_matches('0') == "1"
        }
        None => true,
    }
}

/// Starts UnRAR on `first` with `-vp`: it asks for each next volume, and
/// the worker answers once that volume is in `ready` (or the download is
/// over and the file exists). Cancelling answers "Quit".
pub fn spawn_direct(collection_id: u64, first: PathBuf, source: PathBuf, dest: PathBuf, options: UnrarOptions) -> DirectUnrar {
    let (tx, rx) = mpsc::channel();
    let ready = Arc::new(Mutex::new(vec![first.clone()]));
    let finished = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    let (r, f, c) = (ready.clone(), finished.clone(), cancel.clone());
    std::thread::spawn(move || {
        let _ = tx.send(run_direct(&first, &source, &dest, &options, &r, &f, &c));
    });
    DirectUnrar { collection_id, ready, finished, cancel, rx }
}

fn run_direct(
    first: &Path,
    source: &Path,
    dest: &Path,
    options: &UnrarOptions,
    ready: &Mutex<Vec<PathBuf>>,
    finished: &AtomicBool,
    cancel: &AtomicBool,
) -> Result<String, UnrarError> {
    use std::process::Stdio;
    let exe = find_unrar_exe().ok_or_else(|| UnrarError::Failed("UnRAR.exe not found".into()))?;
    fs::create_dir_all(dest).map_err(|e| UnrarError::Failed(format!("Cannot create unrar folder: {e}")))?;
    let mut command = Command::new(&exe);
    command.arg("x").arg("-y").arg("-vp");
    command.arg(if options.overwrite { "-o+" } else if options.rename_existing { "-or" } else { "-o-" });
    if options.password.trim().is_empty() {
        command.arg("-p-");
    } else {
        command.arg(format!("-p{}", options.password));
    }
    command.arg(first).arg(format!("{}{}", dest.display(), std::path::MAIN_SEPARATOR));
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|e| UnrarError::Failed(format!("Cannot run {}: {e}", exe.display())))?;
    // UnRAR asks for the next volume on stderr; stdout only lists files.
    let mut stdout_pipe = child.stdout.take().unwrap();
    let stdout_thread = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).to_string()
    });
    let stderr_pipe = child.stderr.take().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let volume_ready = |path: &Path| {
        let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
        let listed = ready
            .lock()
            .map(|r| r.iter().any(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()) == name))
            .unwrap_or(false);
        let over = finished.load(Ordering::Relaxed);
        if listed || (over && path.is_file()) {
            Some(true)
        } else if over {
            Some(false)
        } else {
            None
        }
    };
    let (stderr, quit) = answer_prompts(stderr_pipe, &mut stdin, volume_ready, cancel);
    drop(stdin);
    let status = child.wait().map_err(|e| UnrarError::Failed(e.to_string()))?;
    let stdout = stdout_thread.join().unwrap_or_default();
    let lower = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if lower.contains("incorrect password") || lower.contains("enter password") || lower.contains("specified password") {
        return Err(UnrarError::PasswordRequired);
    }
    if let Some(reason) = quit {
        return Err(UnrarError::Failed(reason));
    }
    let code = status.code().unwrap_or(-1);
    if code != 0 && code != 1 {
        let detail = last_nonempty(&stderr).or_else(|| last_nonempty(&stdout)).unwrap_or_else(|| format!("UnRAR exit code {code}"));
        return Err(UnrarError::Failed(format!("Direct unpack failed: {detail}")));
    }
    finish(source, dest, first, options, code, &stdout, &stderr)
}

/// Reads UnRAR's output and answers each "Insert disk with <path>" prompt:
/// `C` once `volume_ready(path)` says yes, `Q` when it says no or on cancel.
/// Returns the output and, when it quit, why.
fn answer_prompts(
    mut out: impl Read,
    input: &mut impl std::io::Write,
    volume_ready: impl Fn(&Path) -> Option<bool>,
    cancel: &AtomicBool,
) -> (String, Option<String>) {
    let mut all = Vec::new();
    let mut line = Vec::new();
    let mut quit = None;
    let mut byte = [0u8; 1];
    while let Ok(1) = out.read(&mut byte) {
        all.push(byte[0]);
        line.push(byte[0]);
        if byte[0] == b'\n' {
            line.clear();
            continue;
        }
        let text = String::from_utf8_lossy(&line).to_string();
        if !text.trim_end().ends_with("[Q]uit") {
            continue;
        }
        line.clear();
        let path = text
            .split("Insert disk with ")
            .nth(1)
            .and_then(|rest| rest.split(" [C]ontinue").next())
            .map(|p| PathBuf::from(p.trim()));
        let answer = loop {
            if cancel.load(Ordering::Relaxed) {
                quit = Some("Direct unpack cancelled".to_string());
                break "Q";
            }
            match path.as_deref().map(&volume_ready) {
                Some(Some(true)) => break "C",
                Some(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                _ => {
                    let name = path.as_deref().map(|p| p.display().to_string()).unwrap_or_default();
                    quit = Some(format!("Volume {name} never arrived"));
                    break "Q";
                }
            }
        };
        let _ = input.write_all(format!("{answer}\n").as_bytes());
        let _ = input.flush();
        if answer == "Q" {
            break;
        }
    }
    let mut rest = Vec::new();
    let _ = out.read_to_end(&mut rest);
    all.extend(rest);
    (String::from_utf8_lossy(&all).to_string(), quit)
}

#[cfg(test)]
mod direct_tests {
    use super::*;

    fn options() -> UnrarOptions {
        UnrarOptions {
            overwrite: true,
            rename_existing: false,
            strip_root: false,
            delete_archives: false,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        }
    }

    fn wait(direct: &DirectUnrar, secs: u64) -> Option<Result<String, UnrarError>> {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(secs) {
            if let Some(r) = direct.poll() {
                return Some(r);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        None
    }

    /// Real UnRAR on a 3-volume set whose volumes "arrive" one by one.
    #[test]
    fn extracts_volumes_as_they_arrive() {
        if !unrar_available() {
            return;
        }
        let fixtures = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/multi"));
        for (case, deliver_last) in [("ok", true), ("short", false)] {
            let root = std::env::temp_dir().join(format!("newsqueue-direct-{case}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let (source, dest) = (root.join("src"), root.join("out"));
            fs::create_dir_all(&source).unwrap();
            let bring = |n: u32| {
                let name = format!("multi.part{n}.rar");
                fs::copy(fixtures.join(&name), source.join(&name)).unwrap();
                source.join(name)
            };
            let first = bring(1);
            let direct = spawn_direct(1, first, source.clone(), dest.clone(), options());
            // UnRAR is now waiting for part2; nothing has finished.
            assert!(wait(&direct, 1).is_none());
            let second = bring(2);
            direct.ready.lock().unwrap().push(second);
            assert!(wait(&direct, 1).is_none());
            if deliver_last {
                let third = bring(3);
                direct.ready.lock().unwrap().push(third);
                let result = wait(&direct, 20).expect("finishes");
                assert!(result.is_ok(), "{result:?}");
                assert_eq!(fs::metadata(dest.join("payload.bin")).unwrap().len(), 150_000);
            } else {
                // The download ended without part3: give up, do not hang.
                direct.finished.store(true, Ordering::Relaxed);
                match wait(&direct, 20).expect("finishes") {
                    Err(UnrarError::Failed(e)) => assert!(e.contains("never arrived"), "{e}"),
                    other => panic!("{other:?}"),
                }
            }
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn first_volume_names() {
        assert!(is_first_rar_volume("a.rar"));
        assert!(is_first_rar_volume("a.part01.rar"));
        assert!(is_first_rar_volume("A.PART001.RAR"));
        assert!(!is_first_rar_volume("a.part02.rar"));
        assert!(!is_first_rar_volume("a.r00"));
    }

    #[test]
    fn prompts_wait_for_ready_volumes() {
        let out = b"Extracting a.rar\nInsert disk with C:\\d\\a.r00 [C]ontinue, [Q]uit Extracting a.r00\nInsert disk with C:\\d\\a.r01 [C]ontinue, [Q]uit ".to_vec();
        let calls = std::cell::Cell::new(0);
        let mut answers = Vec::new();
        let cancel = AtomicBool::new(false);
        let (text, quit) = answer_prompts(&out[..], &mut answers, |p| {
            calls.set(calls.get() + 1);
            match p.file_name().unwrap().to_str().unwrap() {
                // a.r00 is not ready on the first look, then it is.
                "a.r00" if calls.get() == 1 => None,
                "a.r00" => Some(true),
                _ => Some(false),
            }
        }, &cancel);
        assert_eq!(String::from_utf8(answers).unwrap(), "C\nQ\n");
        assert!(quit.unwrap().contains("a.r01"));
        assert!(text.contains("Extracting a.r00"));
        assert_eq!(calls.get(), 3);

        cancel.store(true, Ordering::Relaxed);
        let mut answers = Vec::new();
        let (_, quit) = answer_prompts(&out[..], &mut answers, |_| None, &cancel);
        assert_eq!(answers, b"Q\n");
        assert_eq!(quit.as_deref(), Some("Direct unpack cancelled"));
    }
}

/// True when `source` contains a RAR archive that extraction can start from.
pub fn has_archive(source: &Path, expected: &[String]) -> bool {
    normalize_obfuscated_names(source);
    find_archive(source, expected).is_some()
}

/// Extract the first archive volume found in `source` into `dest`.
pub fn extract(
    source: &Path,
    dest: &Path,
    options: &UnrarOptions,
) -> Result<String, UnrarError> {
    normalize_obfuscated_names(source);
    let archive = find_archive(source, &options.expected_files).ok_or_else(|| {
        UnrarError::Failed(format!("No archive found in {}", source.display()))
    })?;
    if matches!(archive_kind(&archive), Some(Container::SevenZip | Container::Zip)) {
        let (code, stdout, stderr) = run_7z(&archive, dest, options)?;
        return finish(source, dest, &archive, options, code, &stdout, &stderr);
    }
    let exe = find_unrar_exe().ok_or_else(|| {
        UnrarError::Failed(
            "UnRAR.exe not found. Put UnRAR.exe in app/tools/ or install WinRAR.".to_string(),
        )
    })?;
    fs::create_dir_all(dest)
        .map_err(|e| UnrarError::Failed(format!("Cannot create unrar folder: {e}")))?;

    let mut command = Command::new(&exe);
    command.arg("x").arg("-y").arg("-idq").arg("-ierr");
    command.arg(if options.overwrite {
        "-o+"
    } else if options.rename_existing {
        "-or"
    } else {
        "-o-"
    });
    if options.password.trim().is_empty() {
        command.arg("-p-");
    } else {
        command.arg(format!("-p{}", options.password));
    }
    command.arg(&archive);
    // UnRAR treats a trailing separator as "extract into this folder".
    let dest_arg = format!("{}{}", dest.display(), std::path::MAIN_SEPARATOR);
    command.arg(&dest_arg);

    let output = command
        .output()
        .map_err(|e| UnrarError::Failed(format!("Cannot run {}: {e}", exe.display())))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().unwrap_or(-1);
    let combined = format!("{stdout}\n{stderr}");
    let lower = combined.to_ascii_lowercase();
    if lower.contains("incorrect password")
        || lower.contains("enter password")
        || lower.contains("password is incorrect")
        || lower.contains("specified password")
    {
        return Err(UnrarError::PasswordRequired);
    }
    // 0 = success, 1 = non-fatal warning. Everything else is a failure.
    if code != 0 && code != 1 {
        let detail = last_nonempty(&stderr)
            .or_else(|| last_nonempty(&stdout))
            .unwrap_or_else(|| format!("UnRAR exit code {code}"));
        return Err(UnrarError::Failed(format!(
            "Unrar failed for {}: {detail}",
            archive.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    finish(source, dest, &archive, options, code, &stdout, &stderr)
}

/// What the archive is, from its first bytes (names can lie or be missing).
fn archive_kind(archive: &Path) -> Option<Container> {
    use std::io::Read;
    let mut head = [0u8; crate::container::SNIFF_LEN];
    let n = fs::File::open(archive).ok()?.read(&mut head).ok()?;
    crate::container::sniff(&head[..n])
}

/// 7z.exe: `NQ_7Z`, `tools/7z.exe`, 7-Zip's install folder, or `7z` on PATH.
fn find_7z_exe() -> Option<PathBuf> {
    if std::env::var_os("NQ_7Z_OFF").is_some() {
        return None;
    }
    if let Some(p) = std::env::var_os("NQ_7Z").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let mut candidates = vec![
        app_base().join("tools").join("7z.exe"),
        PathBuf::from(r"C:\Program Files\7-Zip\7z.exe"),
        PathBuf::from(r"C:\Program Files (x86)\7-Zip\7z.exe"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["7z.exe", "7za.exe", "7z", "7zz"] {
                candidates.push(dir.join(name));
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// bsdtar (Windows 10+ ships it as `System32\tar.exe`) reads ZIP and 7z.
fn find_bsdtar_exe() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NQ_TAR").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let windir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    [windir.join("System32").join("tar.exe"), PathBuf::from("/usr/bin/bsdtar")]
        .into_iter()
        .find(|p| p.is_file())
}

/// Whether 7z/ZIP archives can be extracted here.
pub fn sevenzip_available() -> bool {
    find_7z_exe().is_some() || find_bsdtar_exe().is_some()
}

/// Extracts a 7z or ZIP archive (first volume of a `.7z.001` set too).
/// Returns (exit code mapped to UnRAR's 0 ok / 1 warning / 2+ error, stdout, stderr).
fn run_7z(archive: &Path, dest: &Path, options: &UnrarOptions) -> Result<(i32, String, String), UnrarError> {
    fs::create_dir_all(dest)
        .map_err(|e| UnrarError::Failed(format!("Cannot create {}: {e}", dest.display())))?;
    let password = options.password.trim();
    let output = if let Some(exe) = find_7z_exe() {
        let mut c = Command::new(&exe);
        c.arg("x").arg("-y").arg("-bso0").arg("-bsp0");
        c.arg(if options.overwrite {
            "-aoa"
        } else if options.rename_existing {
            "-aou"
        } else {
            "-aos"
        });
        // An empty -p makes 7z fail on encryption instead of asking.
        c.arg(format!("-p{password}"));
        c.arg(archive).arg(format!("-o{}", dest.display()));
        c.output().map_err(|e| UnrarError::Failed(format!("Cannot run {}: {e}", exe.display())))?
    } else if let Some(exe) = find_bsdtar_exe() {
        let mut c = Command::new(&exe);
        c.arg("-x");
        if !options.overwrite {
            c.arg("-k");
        }
        if !password.is_empty() {
            c.arg("--passphrase").arg(password);
        }
        c.arg("-f").arg(archive).arg("-C").arg(dest);
        c.output().map_err(|e| UnrarError::Failed(format!("Cannot run {}: {e}", exe.display())))?
    } else {
        return Err(UnrarError::Failed(
            "No 7z/ZIP extractor: put 7z.exe in app/tools/ or install 7-Zip.".into(),
        ));
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let lower = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if lower.contains("wrong password")
        || lower.contains("encrypted archive")
        || lower.contains("passphrase required")
        || lower.contains("couldn't read passphrase")
        || lower.contains("incorrect passphrase")
        || lower.contains("encrypted file is unsupported")
    {
        return Err(UnrarError::PasswordRequired);
    }
    let code = output.status.code().unwrap_or(-1);
    if code != 0 && code != 1 {
        let detail = last_nonempty(&stderr).or_else(|| last_nonempty(&stdout)).unwrap_or_else(|| format!("exit code {code}"));
        return Err(UnrarError::Failed(format!(
            "Extract failed for {}: {detail}",
            archive.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    Ok((code, stdout, stderr))
}

/// The steps after a successful extract, the same for every format.
fn finish(
    source: &Path,
    dest: &Path,
    archive: &Path,
    options: &UnrarOptions,
    code: i32,
    stdout: &str,
    stderr: &str,
) -> Result<String, UnrarError> {
    let archive = archive.to_path_buf();
    if options.strip_root {
        let _ = strip_single_root(dest);
    }
    if options.delete_archives {
        delete_archive_set(&archive);
    }
    move_matching_files(source, dest, &options.move_patterns, options.case_insensitive);
    delete_matching_files(source, &options.delete_patterns, options.case_insensitive);
    if options.auto_rename {
        rename_single_to_collection(dest, &options.collection_name);
    }
    if options.delete_empty_folder {
        remove_empty_dirs(source);
        remove_empty_dirs(dest);
    }

    let extracted = count_files(dest);
    let warning = if code == 1 {
        last_nonempty(stderr)
            .or_else(|| last_nonempty(stdout))
            .map(|line| format!(" ({line})"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok(format!(
        "Unpacked {} file{} to {}{}",
        extracted,
        if extracted == 1 { "" } else { "s" },
        dest.display(),
        warning
    ))
}

fn split_patterns(value: &str) -> Vec<String> {
    value
        .split(['|', ';'])
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Whether an UnRAR executable can be found.
pub fn unrar_available() -> bool {
    find_unrar_exe().is_some()
}

fn find_unrar_exe() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("NQ_UNRAR") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let base = app_base();
    let mut candidates = vec![
        base.join("tools").join("UnRAR.exe"),
        base.join("unrar").join("UnRAR.exe"),
        PathBuf::from("unrar.exe"),
        PathBuf::from(r"C:\Program Files\WinRAR\UnRAR.exe"),
        PathBuf::from(r"C:\Program Files (x86)\WinRAR\UnRAR.exe"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("UnRAR.exe"));
            candidates.push(dir.join("unrar.exe"));
            candidates.push(dir.join("unrar"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn find_archive(source: &Path, expected: &[String]) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(reader) = fs::read_dir(source) {
        for entry in reader.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if is_first_volume(&name) || name.ends_with(".7z") || name.ends_with(".zip") {
                candidates.push(path);
            }
        }
    }
    // RAR sets first: a release may also carry a small .zip (sample, extras).
    let not_rar = |p: &PathBuf| {
        let n = p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
        n.ends_with(".7z") || n.ends_with(".zip") || n.ends_with(".7z.001") || n.ends_with(".zip.001")
    };
    candidates.sort_by(|a, b| {
        let an = a.file_name().unwrap_or_default().to_string_lossy().to_string();
        let bn = b.file_name().unwrap_or_default().to_string_lossy().to_string();
        not_rar(a).cmp(&not_rar(b)).then(an.len().cmp(&bn.len())).then(an.cmp(&bn))
    });
    for candidate in &candidates {
        let name = candidate
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if expected.iter().any(|wanted| wanted.eq_ignore_ascii_case(&name)) {
            return Some(candidate.clone());
        }
    }
    candidates.into_iter().next()
}

/// Renames RAR volumes that arrived without an extension so UnRAR can chain
/// them.
///
/// Obfuscated releases deliver `Rar!` payloads with random names such as
/// `1xnkjchunrcod4zjpbnri`. UnRAR appends `.rar` to whatever it is given and
/// then looks for the other volumes by name, so without a proper set name the
/// archive can never be opened. RAR5 stores the volume number in its main
/// header, which is enough to rebuild `<folder>.partNNN.rar`.
fn normalize_obfuscated_names(source: &Path) {
    let Ok(reader) = fs::read_dir(source) else {
        return;
    };
    let mut volumes: Vec<(u32, PathBuf)> = Vec::new();
    let mut single: Option<PathBuf> = None;
    let mut already_named = false;
    for entry in reader.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if name.ends_with(".rar") || name.ends_with(".rev") || name.ends_with(".001") {
            already_named = true;
            continue;
        }
        let mut head = [0u8; container::SNIFF_LEN];
        let Ok(mut file) = fs::File::open(&path) else {
            continue;
        };
        let Ok(read) = file.read(&mut head) else {
            continue;
        };
        match container::sniff(&head[..read]) {
            Some(container::Container::Rar5 {
                volume: Some(volume),
            }) => volumes.push((volume, path)),
            Some(container::Container::Rar5 { volume: None }) if single.is_none() => {
                single = Some(path)
            }
            _ => {}
        }
    }
    if already_named {
        // A named archive is already present: leave the folder exactly as it is.
        return;
    }
    let base = source
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "archive".to_string());
    if !volumes.is_empty() {
        volumes.sort_by_key(|(volume, _)| *volume);
        for (volume, path) in volumes {
            let target = source.join(container::rar_volume_name(&base, volume));
            if path != target {
                let _ = fs::rename(&path, &target);
            }
        }
    } else if let Some(path) = single {
        let target = source.join(format!("{base}.rar"));
        if path != target {
            let _ = fs::rename(&path, &target);
        }
    }
}

fn is_first_volume(name: &str) -> bool {
    if name.ends_with(".001") {
        return true;
    }
    if !name.ends_with(".rar") || name.ends_with(".part2.rar") {
        return false;
    }
    if let Some(index) = name.rfind(".part") {
        let digits = &name[index + 5..name.len() - 4];
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            let trimmed = digits.trim_start_matches('0');
            return trimmed == "1" || (digits == "1");
        }
    }
    true
}

fn archive_prefix(archive: &Path) -> String {
    let name = archive
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if let Some(index) = name.rfind(".part") {
        return name[..index].to_string();
    }
    if let Some(index) = name.rfind('.') {
        return name[..index].to_string();
    }
    name
}

fn delete_archive_set(archive: &Path) {
    let Some(parent) = archive.parent() else { return };
    let prefix = archive_prefix(archive);
    let prefix_dot = format!("{prefix}.");
    let Ok(reader) = fs::read_dir(parent) else { return };
    for entry in reader.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let Some(suffix) = name.strip_prefix(&prefix_dot) else {
            continue;
        };
        let is_volume = suffix == "rar"
            || suffix == "7z"
            || suffix == "zip"
            || suffix == "rev"
            || (suffix.starts_with("part") && (suffix.ends_with(".rar") || suffix.ends_with(".rev")))
            || (suffix.len() == 3
                && suffix.is_ascii()
                && (suffix.chars().all(|c| c.is_ascii_digit())
                    || ((suffix.as_bytes()[0].eq_ignore_ascii_case(&b'r')
                        || suffix.as_bytes()[0].eq_ignore_ascii_case(&b's'))
                        && suffix[1..].chars().all(|c| c.is_ascii_digit()))));
        if is_volume {
            let _ = fs::remove_file(path);
        }
    }
}

fn strip_single_root(dest: &Path) -> std::io::Result<()> {
    let entries: Vec<_> = fs::read_dir(dest)?.flatten().collect();
    if entries.len() != 1 || !entries[0].file_type()?.is_dir() {
        return Ok(());
    }
    let root = entries[0].path();
    for child in fs::read_dir(&root)?.flatten() {
        let target = dest.join(child.file_name());
        if !target.exists() {
            fs::rename(child.path(), target)?;
        }
    }
    let _ = fs::remove_dir(root);
    Ok(())
}

fn rename_single_to_collection(dest: &Path, collection_name: &str) {
    let Some(name) = sanitize_name(collection_name) else {
        return;
    };
    let Ok(entries) = fs::read_dir(dest) else {
        return;
    };
    let entries: Vec<_> = entries.flatten().collect();
    if entries.len() != 1 || !entries[0].file_type().map(|kind| kind.is_file()).unwrap_or(false) {
        return;
    }
    let source = entries[0].path();
    let extension = source
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let target = dest.join(format!("{name}{extension}"));
    if target != source && !target.exists() {
        let _ = fs::rename(source, target);
    }
}

fn sanitize_name(name: &str) -> Option<String> {
    let name = sanitize_folder(name);
    if name.trim().is_empty() || name == "collection" {
        None
    } else {
        Some(name)
    }
}

fn remove_empty_dirs(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            remove_empty_dirs(&child);
            let empty = fs::read_dir(&child)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if empty {
                let _ = fs::remove_dir(child);
            }
        }
    }
}

fn move_matching_files(source: &Path, dest: &Path, patterns: &[String], ignore_case: bool) {
    if patterns.is_empty() || source == dest {
        return;
    }
    let Ok(reader) = fs::read_dir(source) else { return };
    for entry in reader.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if patterns.iter().any(|pattern| glob_match(pattern, &name, ignore_case)) {
            let target = unique_path(dest.join(&name));
            let _ = fs::rename(path, target);
        }
    }
}

fn delete_matching_files(source: &Path, patterns: &[String], ignore_case: bool) {
    if patterns.is_empty() {
        return;
    }
    let Ok(reader) = fs::read_dir(source) else { return };
    for entry in reader.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if patterns.iter().any(|pattern| glob_match(pattern, &name, ignore_case)) {
            let _ = fs::remove_file(path);
        }
    }
}

fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let extension = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path
}

fn glob_match(pattern: &str, name: &str, ignore_case: bool) -> bool {
    let (pattern, name) = if ignore_case {
        (pattern.to_ascii_lowercase(), name.to_ascii_lowercase())
    } else {
        (pattern.to_string(), name.to_string())
    };
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let mut pi = 0;
    let mut ni = 0;
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn last_nonempty(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn count_files(path: &Path) -> usize {
    let Ok(reader) = fs::read_dir(path) else { return 0 };
    let mut count = 0;
    for entry in reader.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            count += count_files(&entry.path());
        } else if file_type.is_file() {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_first_rar_volumes() {
        assert!(is_first_volume("release.rar"));
        assert!(is_first_volume("release.part1.rar"));
        assert!(is_first_volume("release.part01.rar"));
        assert!(is_first_volume("release.part001.rar"));
        assert!(is_first_volume("release.001"));
        assert!(!is_first_volume("release.part2.rar"));
        assert!(!is_first_volume("release.part002.rar"));
        assert!(!is_first_volume("release.r00"));
    }

    #[test]
    fn wildcard_matching_handles_case_and_stars() {
        assert!(glob_match("*.nfo", "info.NFO", true));
        assert!(glob_match("*.nfo", "info.NFO", false) == false);
        assert!(glob_match("sample?.mkv", "sample1.mkv", true));
        assert!(!glob_match("*.nfo", "info.txt", true));
    }

    #[test]
    fn target_dir_adds_group_and_nzb_subfolders() {
        let mut settings = Settings::default();
        let mut collection = crate::model::sample_queue(&settings).remove(0);
        collection.group = "alt.binaries.test".into();
        collection.subject = "Test.Release".into();
        collection.unrar_folder.clear();
        settings.unrar_to_different = true;
        settings.unrar_dir = "C:\\base".into();
        settings.create_dir_group_name = true;
        settings.create_dir_nzb_name = true;
        let path = target_dir(&settings, &collection);
        assert!(path.ends_with(std::path::Path::new("alt.binaries.test").join("Test.Release")));
    }

    #[test]
    fn auto_renames_single_file_and_removes_empty_folders() {
        if find_unrar_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-rename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(source.join("empty")).unwrap();
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("single.rar");
        fs::copy(fixture, source.join("single.rar")).unwrap();
        let options = UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: true,
            delete_archives: false,
            delete_empty_folder: true,
            auto_rename: true,
            collection_name: "Renamed.Release".into(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        };
        extract(&source, &dest, &options).unwrap();
        assert!(dest.join("Renamed.Release.mkv").is_file());
        assert!(!source.join("empty").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_an_archive_that_arrived_without_an_extension() {
        if find_unrar_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        // Obfuscated posts deliver `Rar!` payloads with random names such as
        // `1xnkjchunrcod4zjpbnri` and no extension at all.
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-obfuscated-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("jada-39768c");
        let dest = root.join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::copy(&fixture, source.join("1xnkjchunrcod4zjpbnri")).unwrap();
        let options = UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: true,
            delete_archives: false,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        };
        assert!(has_archive(&source, &[]));
        assert!(
            source.join("jada-39768c.rar").is_file(),
            "the archive should be renamed after the collection"
        );
        assert!(!source.join("1xnkjchunrcod4zjpbnri").exists());
        let message = extract(&source, &dest, &options).unwrap();
        assert!(message.contains("2 files"), "{message}");
        assert_eq!(
            fs::read_to_string(dest.join("readme.txt")).unwrap().trim(),
            "tiny rar fixture"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renames_obfuscated_rar_volumes_by_their_header_number() {
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-volumes-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("Some.Release");
        fs::create_dir_all(&source).unwrap();
        // Two volumes out of order, named with random obfuscated names.
        for (volume, name) in [(0u8, "zzzzzzzzzzzzzzzzzzzz"), (1u8, "aaaaaaaaaaaaaaaaaaaa")] {
            let mut header = b"Rar!\x1a\x07\x01\x00".to_vec();
            header.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd, 16, 1, 5, 12, 3, volume, 0x0b]);
            header.resize(64, 0);
            fs::write(source.join(name), header).unwrap();
        }
        normalize_obfuscated_names(&source);
        assert!(source.join("Some.Release.part001.rar").is_file());
        assert!(source.join("Some.Release.part002.rar").is_file());
        assert!(!source.join("zzzzzzzzzzzzzzzzzzzz").exists());
        assert!(has_archive(&source, &[]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_a_rar_and_strips_the_root_folder() {
        if find_unrar_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::copy(&fixture, source.join("tiny.rar")).unwrap();
        let options = UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: true,
            delete_archives: false,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        };
        let message = extract(&source, &dest, &options).unwrap();
        assert!(message.contains("2 files"), "{message}");
        assert_eq!(
            fs::read_to_string(dest.join("readme.txt")).unwrap().trim(),
            "tiny rar fixture"
        );
        assert_eq!(
            fs::read_to_string(dest.join("Sub").join("data.bin"))
                .unwrap()
                .trim(),
            "payload"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_name_subfolder_uses_the_first_volume_stem() {
        if find_unrar_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-archive-name-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::copy(fixture, source.join("my-release.rar")).unwrap();
        let mut settings = Settings::default();
        settings.create_dir_archive_name = true;
        settings.create_dir_group_name = false;
        settings.create_dir_nzb_name = false;
        let mut collection = crate::model::sample_queue(&settings).remove(0);
        collection.folder = source.to_string_lossy().to_string();
        collection.unrar_folder = root.join("dest").to_string_lossy().to_string();
        let target = target_dir(&settings, &collection);
        assert!(target.ends_with("my-release"), "{}", target.display());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn password_protected_archive_prompts_then_extracts_with_the_password() {
        password_protected_rar();
    }

    fn plain_options() -> UnrarOptions {
        UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: false,
            delete_archives: true,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        }
    }

    /// Builds `name` from `files` with bsdtar (`format`: zip or 7zip).
    fn make_archive(dir: &Path, name: &str, format: &str, passphrase: Option<&str>) -> bool {
        let Some(tar) = find_bsdtar_exe() else { return false };
        let mut c = Command::new(tar);
        c.current_dir(dir).arg("-c").arg("--format").arg(format);
        if let Some(p) = passphrase {
            c.arg("--options").arg("zip:encryption=zipcrypt").arg("--passphrase").arg(p);
        }
        c.arg("-f").arg(name).arg("inner");
        c.status().map(|s| s.success()).unwrap_or(false)
    }

    fn sevenzip_round_trip(tag: &str, format: &str, archive: &str, use_7z_exe: bool) {
        if find_bsdtar_exe().is_none() || (use_7z_exe && find_7z_exe().is_none()) {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!("newsqueue-7z-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("source");
        fs::create_dir_all(source.join("inner")).unwrap();
        fs::write(source.join("inner").join("movie.mkv"), vec![7u8; 50_000]).unwrap();
        assert!(make_archive(&source, archive, format, None));
        fs::remove_dir_all(source.join("inner")).unwrap();
        // Obfuscated: no extension; the name is repaired from the content later.
        let dest = root.join("dest");
        let opts = plain_options();
        let message = with_extractor(use_7z_exe, || extract(&source, &dest, &opts)).unwrap();
        assert!(message.starts_with("Unpacked 1 file"), "{message}");
        assert_eq!(fs::read(dest.join("inner").join("movie.mkv")).unwrap(), vec![7u8; 50_000]);
        assert!(!source.join(archive).exists(), "delete_archives removes the archive");
        let _ = fs::remove_dir_all(root);
    }

    /// Runs `f` with only 7z.exe (true) or only bsdtar (false) visible.
    fn with_extractor<R>(use_7z_exe: bool, f: impl FnOnce() -> R) -> R {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var_os("NQ_7Z_OFF");
        if use_7z_exe {
            std::env::remove_var("NQ_7Z_OFF");
        } else {
            std::env::set_var("NQ_7Z_OFF", "1");
        }
        let r = f();
        match saved {
            Some(v) => std::env::set_var("NQ_7Z_OFF", v),
            None => std::env::remove_var("NQ_7Z_OFF"),
        }
        r
    }

    #[test]
    fn extracts_zip_and_7z_with_either_tool() {
        sevenzip_round_trip("zip-tar", "zip", "set.zip", false);
        sevenzip_round_trip("7z-tar", "7zip", "set.7z", false);
        sevenzip_round_trip("zip-7z", "zip", "set.zip", true);
        sevenzip_round_trip("7z-7z", "7zip", "set.7z", true);
    }

    #[test]
    fn encrypted_zip_asks_for_the_password() {
        for use_7z_exe in [false, true] {
            if find_bsdtar_exe().is_none() || (use_7z_exe && find_7z_exe().is_none()) {
                continue;
            }
            let root = std::env::temp_dir().join(format!("newsqueue-7z-pw-{use_7z_exe}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let source = root.join("source");
            fs::create_dir_all(source.join("inner")).unwrap();
            fs::write(source.join("inner").join("a.txt"), b"secret contents").unwrap();
            assert!(make_archive(&source, "locked.zip", "zip", Some("pw1")));
            fs::remove_dir_all(source.join("inner")).unwrap();
            let dest = root.join("dest");
            let mut opts = plain_options();
            opts.delete_archives = false;
            assert_eq!(with_extractor(use_7z_exe, || extract(&source, &dest, &opts)), Err(UnrarError::PasswordRequired));
            opts.password = "pw1".into();
            with_extractor(use_7z_exe, || extract(&source, &dest, &opts)).unwrap();
            assert_eq!(fs::read(dest.join("inner").join("a.txt")).unwrap(), b"secret contents");
            let _ = fs::remove_dir_all(root);
        }
    }

    fn password_protected_rar() {
        if find_unrar_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("encrypted.rar");
        let root = std::env::temp_dir().join(format!(
            "newsqueue-unrar-password-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let source = root.join("source");
        let dest = root.join("dest");
        fs::create_dir_all(&source).unwrap();
        fs::copy(fixture, source.join("encrypted.rar")).unwrap();
        let mut options = UnrarOptions {
            overwrite: true,
            rename_existing: true,
            strip_root: false,
            delete_archives: false,
            delete_empty_folder: false,
            auto_rename: false,
            collection_name: String::new(),
            password: String::new(),
            move_patterns: Vec::new(),
            delete_patterns: Vec::new(),
            case_insensitive: true,
            expected_files: Vec::new(),
        };
        assert_eq!(
            extract(&source, &dest, &options),
            Err(UnrarError::PasswordRequired)
        );
        options.password = "secret".into();
        extract(&source, &dest, &options).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("payload.txt")).unwrap().trim(),
            "secret payload"
        );
        let _ = fs::remove_dir_all(root);
    }
}
