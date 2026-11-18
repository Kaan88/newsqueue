//! PAR2 verification and repair for completed collections.
//!
//! The original AltBinz bundles `par2.exe`; this build keeps a copy in
//! `tools/par2.exe` and runs it in a worker thread so the UI never blocks.

use crate::model::{app_base, Settings};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub struct ActivePar2 {
    pub collection_id: u64,
    pub rx: Receiver<Result<String, String>>,
}

#[derive(Clone, Debug)]
pub struct Par2Options {
    pub repair: bool,
    pub purge: bool,
    /// The file names the NZB listed. A PAR2 set often also covers files
    /// that were never posted (an `.nfo`); those are reported as "not in
    /// the NZB" rather than as damage. Empty: no such distinction.
    pub nzb_files: Vec<String>,
    /// Held recovery volumes exist: when blocks are short, report
    /// `NEED_BLOCKS` instead of failing, so they can be fetched.
    pub fetch_more: bool,
}

/// Error prefix: the set needs this many more recovery blocks.
pub const NEED_BLOCKS: &str = "PAR2 needs more recovery blocks: ";

/// "You need 12 more recovery blocks to be able to repair."
pub fn blocks_needed(output: &str) -> Option<u32> {
    let lower = output.to_ascii_lowercase();
    let at = lower.find("you need ")?;
    lower[at + 9..].split_whitespace().next()?.parse().ok()
}

/// Recovery blocks in a volume named `x.vol003+004.par2` (4).
pub fn volume_blocks(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let at = lower.rfind(".vol")?;
    let rest = &lower[at + 4..];
    let plus = rest.find('+')?;
    let end = rest[plus + 1..].find(|c: char| !c.is_ascii_digit())? + plus + 1;
    rest[plus + 1..end].parse().ok()
}

/// Indexes of `volumes` (index, blocks) to fetch for `need` blocks: the
/// fewest blocks that cover the need, or all of them when they cannot.
pub fn pick_volumes(volumes: &[(usize, u32)], need: u32) -> Vec<usize> {
    let total: u32 = volumes.iter().map(|v| v.1).sum();
    if total <= need {
        return volumes.iter().map(|v| v.0).collect();
    }
    // Small sets: exact search for the least overshoot; else greedy.
    if volumes.len() <= 16 {
        let mut best: Option<(u32, u32)> = None; // (blocks, mask)
        for mask in 1u32..(1 << volumes.len()) {
            let sum: u32 = (0..volumes.len()).filter(|i| mask & (1 << i) != 0).map(|i| volumes[i].1).sum();
            if sum >= need && best.map(|b| sum < b.0).unwrap_or(true) {
                best = Some((sum, mask));
            }
        }
        if let Some((_, mask)) = best {
            return (0..volumes.len()).filter(|i| mask & (1 << i) != 0).map(|i| volumes[i].0).collect();
        }
    }
    let mut sorted = volumes.to_vec();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let mut picked = Vec::new();
    let mut sum = 0;
    for (i, blocks) in sorted {
        if sum >= need {
            break;
        }
        picked.push(i);
        sum += blocks;
    }
    picked
}

impl Par2Options {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            repair: settings.par2_repairing,
            purge: settings.cleanup_after_repair,
            nzb_files: Vec::new(),
            fetch_more: false,
        }
    }

    pub fn with_fetch_more(mut self, fetch_more: bool) -> Self {
        self.fetch_more = fetch_more;
        self
    }

    pub fn with_nzb_files(mut self, files: Vec<String>) -> Self {
        self.nzb_files = files;
        self
    }
}

/// A file par2 found missing or damaged.
#[derive(Clone, Debug, PartialEq)]
pub struct BadTarget {
    pub name: String,
    pub missing: bool,
}

/// Reads `Target: "name" - missing.` / `- damaged. ...` lines.
pub fn bad_targets(output: &str) -> Vec<BadTarget> {
    output
        .split(['\r', '\n'])
        .filter_map(|line| {
            // Progress output ("Scanning: 3%") can share the line.
            let start = line.find("Target: \"")?;
            let rest = &line[start + "Target: \"".len()..];
            let (name, state) = rest.split_once("\" - ")?;
            let missing = state.starts_with("missing");
            (missing || state.starts_with("damaged")).then(|| BadTarget { name: name.to_string(), missing })
        })
        .collect()
}

/// Names among `bad` that the NZB did not list (empty when `nzb_files` is).
fn not_in_nzb(bad: &[BadTarget], nzb_files: &[String]) -> Vec<String> {
    if nzb_files.is_empty() {
        return Vec::new();
    }
    bad.iter()
        .filter(|t| !nzb_files.iter().any(|f| f.eq_ignore_ascii_case(&t.name)))
        .map(|t| t.name.clone())
        .collect()
}

pub fn spawn(
    collection_id: u64,
    source: PathBuf,
    par2_file: PathBuf,
    options: Par2Options,
) -> ActivePar2 {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = verify_or_repair(&source, &par2_file, &options);
        let _ = tx.send(result);
    });
    ActivePar2 { collection_id, rx }
}

pub fn poll(active: &ActivePar2) -> Option<Result<String, String>> {
    match active.rx.try_recv() {
        Ok(value) => Some(value),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => {
            Some(Err("PAR2 worker stopped unexpectedly".into()))
        }
    }
}

pub fn has_par2(source: &Path, expected: &[String]) -> bool {
    find_par2_file(source, expected).is_some()
}

pub fn find_par2_file(source: &Path, expected: &[String]) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let reader = fs::read_dir(source).ok()?;
    for entry in reader.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.to_ascii_lowercase().ends_with(".par2") {
            candidates.push(path);
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
    candidates
        .iter()
        .find(|path| !is_volume_file(path))
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn is_volume_file(path: &Path) -> bool {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains(".vol")
}

pub fn verify_or_repair(
    source: &Path,
    par2_file: &Path,
    options: &Par2Options,
) -> Result<String, String> {
    let exe = find_par2_exe().ok_or_else(|| {
        "par2.exe not found. Put par2.exe in app/tools/ or install par2cmdline.".to_string()
    })?;
    let par2_name = par2_file
        .file_name()
        .ok_or_else(|| "PAR2 path has no file name".to_string())?
        .to_string_lossy()
        .to_string();

    let (code, output) = run_par2(&exe, source, &["v", "-q", &par2_name])?;
    if code == 0 {
        return Ok(format!("PAR2 verified: {par2_name}"));
    }
    let bad = bad_targets(&output);
    let outside = not_in_nzb(&bad, &options.nzb_files);
    // Only files that were never posted are absent: the download is complete.
    let only_outside = !bad.is_empty() && outside.len() == bad.len() && bad.iter().all(|t| t.missing);
    if only_outside && !options.repair {
        return Ok(format!(
            "PAR2 verified: {par2_name}; not in the NZB and not downloaded: {}",
            outside.join(", ")
        ));
    }
    if !options.repair {
        return Err(format!(
            "PAR2 damage found in {par2_name} and repair is disabled: {}",
            tail(&output)
        ));
    }
    if let Some(n) = blocks_needed(&output) {
        if options.fetch_more {
            return Err(format!("{NEED_BLOCKS}{n}"));
        }
    }
    if output.to_ascii_lowercase().contains("repair is not possible") {
        return Err(format!(
            "PAR2 could not repair {par2_name}: {}",
            tail(&output)
        ));
    }

    let mut repair_args = vec!["r", "-q"];
    if options.purge {
        repair_args.push("-p");
    }
    repair_args.push(&par2_name);
    let (repair_code, repair_output) = run_par2(&exe, source, &repair_args)?;
    if repair_code == 0 && only_outside {
        Ok(format!(
            "PAR2 verified: {par2_name}; rebuilt files not in the NZB: {}",
            outside.join(", ")
        ))
    } else if repair_code == 0 && !outside.is_empty() {
        Ok(format!(
            "PAR2 repaired: {par2_name} (also rebuilt files not in the NZB: {})",
            outside.join(", ")
        ))
    } else if repair_code == 0 {
        Ok(format!("PAR2 repaired: {par2_name}"))
    } else {
        Err(format!(
            "PAR2 repair failed for {par2_name}: {}",
            tail(&repair_output)
        ))
    }
}

fn run_par2(exe: &Path, source: &Path, args: &[&str]) -> Result<(i32, String), String> {
    let output = Command::new(exe)
        .args(args)
        .current_dir(source)
        .output()
        .map_err(|e| format!("Cannot run {}: {e}", exe.display()))?;
    let code = output.status.code().unwrap_or(-1);
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    Ok((code, text))
}

fn find_par2_exe() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("NQ_PAR2") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let base = app_base();
    let candidates = [
        base.join("tools").join("par2.exe"),
        base.join("par2").join("par2.exe"),
        PathBuf::from("par2.exe"),
        PathBuf::from(r"C:\Program Files\par2cmdline\par2.exe"),
    ];
    let mut candidates = candidates.to_vec();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("par2.exe"));
            candidates.push(dir.join("par2"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn tail(text: &str) -> String {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn damages_and_repairs_a_file() {
        if find_par2_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "newsqueue-par2-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let payload = root.join("payload.rar");
        let mut data = vec![0u8; 64 * 1024];
        for (index, byte) in data.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(31) % 251) as u8;
        }
        let mut file = fs::File::create(&payload).unwrap();
        file.write_all(&data).unwrap();
        drop(file);

        let exe = find_par2_exe().expect("par2.exe missing");
        let output = Command::new(&exe)
            .args(["c", "-q", "-r10", "payload.par2", "payload.rar"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));

        let mut damaged = fs::read(&payload).unwrap();
        damaged[100] ^= 0xff;
        fs::write(&payload, damaged).unwrap();

        let par2_file = root.join("payload.par2");
        let options = Par2Options {
            repair: true,
            purge: false,
            nzb_files: Vec::new(),
            fetch_more: false,
        };
        let message = verify_or_repair(&root, &par2_file, &options).unwrap();
        assert!(message.contains("repaired"), "{message}");
        assert_eq!(fs::read(&payload).unwrap(), data);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn on_demand_helpers() {
        assert_eq!(volume_blocks("Rel.vol003+004.par2"), Some(4));
        assert_eq!(volume_blocks("Rel.VOL00+01.PAR2"), Some(1));
        assert_eq!(volume_blocks("Rel.par2"), None);
        assert_eq!(volume_blocks("Rel.volume.par2"), None);
        assert_eq!(blocks_needed("Repair is not possible.\r\nYou need 12 more recovery blocks to be able to repair."), Some(12));
        assert_eq!(blocks_needed("All files are correct"), None);
        let vols = [(3, 1), (4, 2), (5, 4), (6, 8)];
        assert_eq!(pick_volumes(&vols, 3), vec![3, 4]);
        assert_eq!(pick_volumes(&vols, 4), vec![5]);
        assert_eq!(pick_volumes(&vols, 99), vec![3, 4, 5, 6]);
    }

    #[test]
    fn missing_volumes_are_reported_as_blocks_to_fetch() {
        if find_par2_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!("newsqueue-par2-od-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data: Vec<u8> = (0..60_000u32).map(|i| (i.wrapping_mul(13) % 251) as u8).collect();
        fs::write(root.join("payload.rar"), &data).unwrap();
        let exe = find_par2_exe().unwrap();
        let out = Command::new(&exe)
            .args(["c", "-q", "-s4000", "-r20", "payload.par2", "payload.rar"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
        // Hold every recovery volume back, as the downloader does.
        let held = root.join("held");
        fs::create_dir_all(&held).unwrap();
        for e in fs::read_dir(&root).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if volume_blocks(&name).is_some() {
                fs::rename(e.path(), held.join(&name)).unwrap();
            }
        }
        let mut damaged = data.clone();
        damaged[10] ^= 0xff;
        damaged[30_000] ^= 0xff;
        fs::write(root.join("payload.rar"), &damaged).unwrap();
        let par2_file = root.join("payload.par2");
        let opts = Par2Options { repair: true, purge: false, nzb_files: Vec::new(), fetch_more: true };
        let err = verify_or_repair(&root, &par2_file, &opts).unwrap_err();
        assert!(err.starts_with(NEED_BLOCKS), "{err}");
        let need: u32 = err[NEED_BLOCKS.len()..].parse().unwrap();
        assert_eq!(need, 2);
        // Bring back just enough volumes, then the repair succeeds.
        let vols: Vec<(usize, u32, String)> = fs::read_dir(&held)
            .unwrap()
            .flatten()
            .enumerate()
            .map(|(i, e)| {
                let n = e.file_name().to_string_lossy().to_string();
                (i, volume_blocks(&n).unwrap(), n)
            })
            .collect();
        let picked = pick_volumes(&vols.iter().map(|v| (v.0, v.1)).collect::<Vec<_>>(), need);
        let fetched: u32 = vols.iter().filter(|v| picked.contains(&v.0)).map(|v| v.1).sum();
        assert!(fetched >= need && fetched < vols.iter().map(|v| v.1).sum::<u32>());
        for v in vols.iter().filter(|v| picked.contains(&v.0)) {
            fs::rename(held.join(&v.2), root.join(&v.2)).unwrap();
        }
        let message = verify_or_repair(&root, &par2_file, &opts).unwrap();
        assert!(message.contains("repaired"), "{message}");
        assert_eq!(fs::read(root.join("payload.rar")).unwrap(), data);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_bad_targets_from_par2_output() {
        let out = "Loading \"set.par2\".\r\r\nScanning: 1%Target: \"x.nfo\" - missing.\r\r\nTarget: \"a.rar\" - damaged. Found 1749 of 1750 data blocks.\r\r\nTarget: \"b.rar\" - found.\r\r\nRepair is required.";
        assert_eq!(
            bad_targets(out),
            vec![
                BadTarget { name: "x.nfo".into(), missing: true },
                BadTarget { name: "a.rar".into(), missing: false },
            ]
        );
        let bad = bad_targets(out);
        assert_eq!(not_in_nzb(&bad, &["A.RAR".into()]), vec!["x.nfo".to_string()]);
        assert!(not_in_nzb(&bad, &[]).is_empty());
    }

    #[test]
    fn a_file_missing_from_the_nzb_is_not_damage() {
        if find_par2_exe().is_none() {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!("newsqueue-par2-nfo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let data: Vec<u8> = (0..40_000u32).map(|i| (i.wrapping_mul(37) % 251) as u8).collect();
        fs::write(root.join("payload.rar"), &data).unwrap();
        fs::write(root.join("release.nfo"), b"release notes that were never posted").unwrap();
        let exe = find_par2_exe().unwrap();
        let output = Command::new(&exe)
            .args(["c", "-q", "-r10", "payload.par2", "payload.rar", "release.nfo"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
        fs::remove_file(root.join("release.nfo")).unwrap();
        let par2_file = root.join("payload.par2");
        let nzb = vec!["payload.rar".to_string(), "payload.par2".to_string()];
        // Repair off: still a clean result, the NFO is named.
        let check = Par2Options { repair: false, purge: false, nzb_files: nzb.clone(), fetch_more: false };
        let message = verify_or_repair(&root, &par2_file, &check).unwrap();
        assert!(message.contains("not in the NZB") && message.contains("release.nfo"), "{message}");
        assert!(!message.contains("repaired"), "{message}");
        // Repair on: the NFO is rebuilt, reported the same way.
        let repair = Par2Options { repair: true, purge: false, nzb_files: nzb, fetch_more: false };
        let message = verify_or_repair(&root, &par2_file, &repair).unwrap();
        assert!(message.contains("rebuilt files not in the NZB: release.nfo"), "{message}");
        assert!(!message.contains("repaired"), "{message}");
        assert!(root.join("release.nfo").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repairs_a_damaged_rar_and_then_unrars_it() {
        if find_par2_exe().is_none() || crate::unrar::unrar_available() == false {
            eprintln!("skipped: tool not found");
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "newsqueue-par2-rar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).unwrap();
        let fixture = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .join("tests")
            .join("fixtures")
            .join("tiny.rar");
        let payload = root.join("payload.rar");
        fs::copy(&fixture, &payload).unwrap();
        let exe = find_par2_exe().expect("par2.exe missing");
        let output = Command::new(&exe)
            .args(["c", "-q", "-r20", "payload.par2", "payload.rar"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));

        let mut damaged = fs::read(&payload).unwrap();
        let index = damaged.len() / 2;
        damaged[index] ^= 0xff;
        fs::write(&payload, damaged).unwrap();

        let par2_file = root.join("payload.par2");
        let options = Par2Options {
            repair: true,
            purge: false,
            nzb_files: Vec::new(),
            fetch_more: false,
        };
        let message = verify_or_repair(&root, &par2_file, &options).unwrap();
        assert!(message.contains("repaired"), "{message}");

        let dest = root.join("extracted");
        let unrar_options = crate::unrar::UnrarOptions {
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
        crate::unrar::extract(&root, &dest, &unrar_options).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("readme.txt")).unwrap().trim(),
            "tiny rar fixture"
        );
        let _ = fs::remove_dir_all(root);
    }
}
