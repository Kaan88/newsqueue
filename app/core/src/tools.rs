//! Paths of the external tools (GUI_PLAN Phase 0 step 6). The settings
//! `unrar_path`, `par2_path` and `sevenzip_path` win when set; empty means
//! "find it" (`NQ_UNRAR`/`NQ_PAR2`/`NQ_7Z`, `tools/`, the usual install
//! folders, then PATH: `UnRAR.exe`/`par2.exe` on Windows, `unrar`/`par2` on
//! Linux).

use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Unrar,
    Par2,
    SevenZip,
}

static OVERRIDES: RwLock<[String; 3]> = RwLock::new([String::new(), String::new(), String::new()]);

fn slot(tool: Tool) -> usize {
    match tool {
        Tool::Unrar => 0,
        Tool::Par2 => 1,
        Tool::SevenZip => 2,
    }
}

/// Takes the paths from the settings (called by the engine every tick).
pub fn configure(unrar: &str, par2: &str, sevenzip: &str) {
    let wanted = [unrar.trim(), par2.trim(), sevenzip.trim()];
    if let Ok(current) = OVERRIDES.read() {
        if current.iter().zip(wanted).all(|(a, b)| a == b) {
            return;
        }
    }
    if let Ok(mut current) = OVERRIDES.write() {
        *current = wanted.map(str::to_string);
    }
}

/// The configured path for `tool`, if one is set and exists.
pub fn configured(tool: Tool) -> Option<PathBuf> {
    pick(OVERRIDES.read().ok()?.get(slot(tool))?)
}

fn pick(path: &str) -> Option<PathBuf> {
    (!path.is_empty()).then(|| PathBuf::from(path)).filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_tool_wins_only_when_it_exists() {
        // Engines in other tests configure the shared table at the same
        // time, so this checks the rule itself.
        let exe = std::env::current_exe().unwrap();
        assert_eq!(pick(&exe.to_string_lossy()), Some(exe));
        assert_eq!(pick(""), None);
        assert_eq!(pick("C:/no/such/7z.exe"), None);
        assert_eq!(slot(Tool::SevenZip), 2);
    }
}
