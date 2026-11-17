//! RAR extraction for completed collections.
//!
//! The original loads `unrar.dll`. This build is 64-bit and the bundled
//! AltBinz DLL is 32-bit, so extraction runs the standalone `UnRAR.exe`
//! (vendored in `tools/`) in a worker thread. The UI polls a channel, just
//! like the NNTP downloader.

use crate::container;
use crate::model::{app_base, sanitize_folder, Collection, Settings};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

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
        UnrarError::Failed(format!("No RAR archive found in {}", source.display()))
    })?;
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
    let dest_arg = format!("{}\\", dest.display());
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
        last_nonempty(&stderr)
            .or_else(|| last_nonempty(&stdout))
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
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(';') {
            candidates.push(Path::new(dir).join("UnRAR.exe"));
            candidates.push(Path::new(dir).join("unrar.exe"));
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
            if is_first_volume(&name) {
                candidates.push(path);
            }
        }
    }
    candidates.sort_by(|a, b| {
        let an = a.file_name().unwrap_or_default().to_string_lossy().to_string();
        let bn = b.file_name().unwrap_or_default().to_string_lossy().to_string();
        an.len().cmp(&bn.len()).then(an.cmp(&bn))
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

    #[cfg(target_os = "windows")]
    #[test]
    fn auto_renames_single_file_and_removes_empty_folders() {
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

    #[cfg(target_os = "windows")]
    #[test]
    fn extracts_an_archive_that_arrived_without_an_extension() {
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

    #[cfg(target_os = "windows")]
    #[test]
    fn extracts_a_rar_and_strips_the_root_folder() {
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

    #[cfg(target_os = "windows")]
    #[test]
    fn archive_name_subfolder_uses_the_first_volume_stem() {
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

    #[cfg(target_os = "windows")]
    #[test]
    fn password_protected_archive_prompts_then_extracts_with_the_password() {
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
