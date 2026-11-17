//! Version check and self-update from the project's GitHub releases.
//!
//! A check asks the GitHub API for the latest release and compares its tag
//! with [`VERSION`]. An update downloads `NewsQueue-<v>-Setup.exe` plus its
//! `.sha256`, verifies the hash and hands back the path; the app then starts
//! the installer silently into its own folder and exits. The installer waits
//! for the old process (`NQ_WAIT_PID`) and starts the new one (`NQ_RELAUNCH`).
//! Network work runs on a worker thread; the UI polls a channel.

use crate::model::{HOME_REPO, VERSION};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub setup_url: String,
    pub sha256_url: String,
}

#[derive(Debug)]
pub enum UpdateEvent {
    /// The newest release is not newer than this build.
    UpToDate(String),
    Available(Release),
    /// The installer was downloaded and verified at this path.
    Downloaded(Release, PathBuf),
    Failed(String),
}

fn agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new();
    if let Ok(tls) = native_tls::TlsConnector::new() {
        builder = builder.tls_connector(std::sync::Arc::new(tls));
    }
    builder
        .timeout(Duration::from_secs(60))
        .user_agent(&format!("NewsQueue/{VERSION}"))
        .build()
}

/// `v1.2.3` / `1.2.3` → `(1, 2, 3)`; missing parts count as 0.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = text.split('.').map(|part| {
        part.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u32>()
    });
    let major = parts.next()?.ok()?;
    let minor = parts.next().map(|p| p.unwrap_or(0)).unwrap_or(0);
    let patch = parts.next().map(|p| p.unwrap_or(0)).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Reads the GitHub "latest release" JSON.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Bad release data: {e}"))?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or("Release has no tag")?
        .to_string();
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    let page = value["html_url"].as_str().unwrap_or_default().to_string();
    let setup_name = format!("NewsQueue-{version}-Setup.exe");
    let sha_name = format!("{setup_name}.sha256");
    let mut setup_url = String::new();
    let mut sha256_url = String::new();
    for asset in value["assets"].as_array().into_iter().flatten() {
        let name = asset["name"].as_str().unwrap_or_default();
        let url = asset["browser_download_url"].as_str().unwrap_or_default();
        if name.eq_ignore_ascii_case(&setup_name) {
            setup_url = url.to_string();
        } else if name.eq_ignore_ascii_case(&sha_name) {
            sha256_url = url.to_string();
        }
    }
    Ok(Release {
        version,
        page,
        setup_url,
        sha256_url,
    })
}

fn fetch_latest() -> Result<Release, String> {
    // NQ_UPDATE_REPO (owner/name) is a diagnostic override for testing.
    let repo = std::env::var("NQ_UPDATE_REPO").unwrap_or_else(|_| HOME_REPO.to_string());
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let body = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => format!(
                "Version check failed: no public release found for {repo} (is the repository private?)"
            ),
            other => format!("Version check failed: {other}"),
        })?
        .into_string()
        .map_err(|e| format!("Version check failed: {e}"))?;
    parse_release(&body)
}

pub fn spawn_check() -> Receiver<UpdateEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let event = match fetch_latest() {
            Ok(release) if is_newer(&release.version, VERSION) => UpdateEvent::Available(release),
            Ok(release) => UpdateEvent::UpToDate(release.version),
            Err(error) => UpdateEvent::Failed(error),
        };
        let _ = tx.send(event);
    });
    rx
}

/// The hex digest from a `sha256sum`-style line (`<hex>  <file>`).
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Download failed: {e}"))?;
    Ok(bytes)
}

fn fetch_installer(release: &Release, folder: &Path) -> Result<PathBuf, String> {
    if release.setup_url.is_empty() || release.sha256_url.is_empty() {
        return Err(format!(
            "Release {} has no installer and checksum to download",
            release.version
        ));
    }
    let expected = String::from_utf8_lossy(&download(&release.sha256_url)?).to_string();
    let expected = parse_sha256_file(&expected).ok_or("The release checksum file is unreadable")?;
    let setup = download(&release.setup_url)?;
    let actual = sha256_hex(&setup);
    if actual != expected {
        return Err(format!(
            "The downloaded installer failed its checksum (expected {expected}, got {actual})"
        ));
    }
    std::fs::create_dir_all(folder).map_err(|e| format!("Cannot create {}: {e}", folder.display()))?;
    let path = folder.join(format!("NewsQueue-{}-Setup.exe", release.version));
    std::fs::write(&path, setup).map_err(|e| format!("Cannot save the installer: {e}"))?;
    Ok(path)
}

pub fn spawn_download(release: Release) -> Receiver<UpdateEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let folder = std::env::temp_dir().join("newsqueue-update");
        let event = match fetch_installer(&release, &folder) {
            Ok(path) => UpdateEvent::Downloaded(release, path),
            Err(error) => UpdateEvent::Failed(error),
        };
        let _ = tx.send(event);
    });
    rx
}

/// The folder this copy runs from, when it is an installed or portable copy
/// that may update itself. Development builds under `target\` never do.
pub fn install_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let lower = exe.to_string_lossy().to_ascii_lowercase();
    if lower.contains("\\target\\") || lower.contains("/target/") {
        return None;
    }
    exe.parent().map(Path::to_path_buf)
}

/// Starts the verified installer silently into `dir`. It waits for this
/// process to exit, installs, then relaunches NewsQueue.
pub fn launch_installer(setup: &Path, dir: &Path) -> Result<(), String> {
    std::process::Command::new(setup)
        .env("NQ_SILENT", "1")
        .env("NQ_NO_DESKTOP", "1")
        .env("NQ_INSTALL_DIR", dir)
        .env("NQ_WAIT_PID", std::process::id().to_string())
        .env("NQ_RELAUNCH", "1")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Cannot start the installer: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v1.0.10", "1.0.9"));
        assert!(is_newer("1.1", "1.0.9"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(!is_newer("v1.0.1", "1.0.1"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("garbage", "1.0.1"));
        assert_eq!(parse_version("v1.2.3-beta"), Some((1, 2, 3)));
    }

    #[test]
    fn reads_the_latest_release_json() {
        let json = r#"{
            "tag_name": "v1.2.0",
            "html_url": "https://github.com/Kaan88/newsqueue/releases/tag/v1.2.0",
            "assets": [
                {"name": "NewsQueue-1.2.0.zip", "browser_download_url": "https://x/zip"},
                {"name": "NewsQueue-1.2.0-Setup.exe", "browser_download_url": "https://x/setup"},
                {"name": "NewsQueue-1.2.0-Setup.exe.sha256", "browser_download_url": "https://x/sha"}
            ]
        }"#;
        let release = parse_release(json).unwrap();
        assert_eq!(release.version, "1.2.0");
        assert_eq!(release.setup_url, "https://x/setup");
        assert_eq!(release.sha256_url, "https://x/sha");
        assert!(release.page.ends_with("v1.2.0"));
    }

    #[test]
    fn checks_the_installer_hash() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let line = "2CE6C619B009D6506F7454F96E1CF7C631A3D263FB92C5288FF870AA32029172  NewsQueue-1.0.1-Setup.exe";
        assert_eq!(
            parse_sha256_file(line).as_deref(),
            Some("2ce6c619b009d6506f7454f96e1cf7c631a3d263fb92c5288ff870aa32029172")
        );
        assert_eq!(parse_sha256_file("not a hash"), None);
    }
}
