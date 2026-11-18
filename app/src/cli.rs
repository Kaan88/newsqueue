//! Command-line diagnostics (`--rss-test`, `--par2-test`, ...). They run
//! before any window is created; debug builds print to the console.

use crate::update;
use newsqueue_core::model::*;
use newsqueue_core::{download, par2, rss, unrar};
use std::path::{Path, PathBuf};

/// Runs a diagnostic named in `args`; `None` when there is none.
pub fn run(args: &[String]) -> Option<i32> {
    match run_inner(args) {
        Some(Ok(())) => Some(0),
        Some(Err(e)) => {
            eprintln!("{e}");
            Some(1)
        }
        None => None,
    }
}

fn run_inner(args: &[String]) -> Option<Result<(), String>> {
    if args.iter().any(|arg| arg == "--rss-download-test") {
        let settings = Settings::load();
        match rss::fetch_and_download(
            &settings.rss_urls,
            &settings.rss_filters,
            Path::new(&settings.nzb_dir),
            &[],
        ) {
            Ok(result) => {
                println!(
                    "{} RSS item(s), {} NZB download(s)",
                    result.items.len(),
                    result.downloads.len()
                );
                for error in &result.errors {
                    println!("error: {error}");
                }
                for download in result.downloads {
                    println!("{} | {} bytes", download.nzb.name, download.nzb.total_bytes);
                }
                return Some(Ok(()));
            }
            Err(error) => {
                return Some(Err(format!("rss download test failed: {error}")));
            }
        }
    }
    if args.iter().any(|arg| arg == "--rss-test") {
        let settings = Settings::load();
        match rss::fetch_feeds(&settings.rss_urls) {
            Ok(items) => {
                println!("{} RSS item(s)", items.len());
                for item in items.iter().take(10) {
                    println!("{} | {}", item.name, item.url);
                }
                return Some(Ok(()));
            }
            Err(error) => {
                return Some(Err(format!("rss test failed: {error}")));
            }
        }
    }
    if args.iter().any(|a| a == "--update-test") {
        // Checks GitHub for a newer release; never downloads or installs.
        match update::spawn_check().recv() {
            Ok(update::UpdateEvent::UpToDate(latest)) => {
                println!("{APP_NAME} {VERSION} is up to date (latest release {latest})");
                return Some(Ok(()));
            }
            Ok(update::UpdateEvent::Available(release)) => {
                println!(
                    "Version {} is available: setup {} / sha256 {}",
                    release.version,
                    if release.setup_url.is_empty() { "missing" } else { "found" },
                    if release.sha256_url.is_empty() { "missing" } else { "found" }
                );
                return Some(Ok(()));
            }
            Ok(other) => {
                return Some(Err(format!("update test failed: {other:?}")));
            }
            Err(error) => {
                return Some(Err(format!("update test failed: {error}")));
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--par2-test") {
        let source = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| ".".into());
        let source = PathBuf::from(source);
        let settings = Settings::load();
        let Some(par2_file) = par2::find_par2_file(&source, &[]) else {
            return Some(Err(format!("No PAR2 file found in {}", source.display())));
        };
        match par2::verify_or_repair(
            &source,
            &par2_file,
            &par2::Par2Options::from_settings(&settings),
        ) {
            Ok(message) => {
                println!("{message}");
                return Some(Ok(()));
            }
            Err(error) => {
                return Some(Err(format!("par2 test failed: {error}")));
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--unrar-test") {
        let source = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| ".".into());
        let dest = args
            .get(position + 2)
            .cloned()
            .unwrap_or_else(|| "unrar-test-output".into());
        let options = unrar::UnrarOptions {
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
        match unrar::extract(Path::new(&source), Path::new(&dest), &options) {
            Ok(message) => {
                println!("{message}");
                return Some(Ok(()));
            }
            Err(error) => {
                return Some(Err(format!("unrar test failed: {error}")));
            }
        }
    }
    if let Some(position) = args.iter().position(|a| a == "--download-test") {
        let path = args
            .get(position + 1)
            .cloned()
            .unwrap_or_else(|| "dancing-4c5722.nzb".into());
        match download::run_download_test(&path) {
            Ok(()) => return Some(Ok(())),
            Err(e) => {
                return Some(Err(format!("download test failed: {e}")));
            }
        }
    }
    None
}
