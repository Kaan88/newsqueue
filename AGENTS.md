# AGENTS.md

## Project

- Read `CLAUDE.md` for the full architecture, verification and release guide, and `ROADMAP.md` for current work and blockers.
- `app/` is the whole Rust 2021 workspace: the native Win32 desktop app (`newsqueue`, root package), `core` (engine), `server` (JSON API library + headless binary) and `web` (egui web UI for WebAssembly). A from-scratch clone of AltBinz, presented as NewsQueue 1.0.5. The web UI must stay egui.
- `app/MECHANICS.md` records measured constants, formulas and function addresses from the original Delphi binary; `app/mechanics.json` is the machine-checked list. Keep both in sync with `model.rs`, and only mark a value "estimated" after searching the evidence.
- `capture/`, `investigation.cany/`, `altbinz_original/` and `reference/` are read-only evidence. Harness files (`craft.json`, `feed.json`, `build-activity.log`, `checks/`) are generated; don't edit them. 
- Ghidra 12.1.4 is installed at `C:\Users\kaank\AppData\Roaming\craft-anything\engines\ghidra\ghidra_12.1.4_PUBLIC`. Use it when the recorded evidence is insufficient to verify a value or behavior; do not guess mechanics.

## Commands (from `app/`)

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

- Single test: `cargo test speed_limit_uses_decompiled_formula`; also `cargo test unrar`, `cargo test par2`, and `cargo test par2_repair_then_unrar_chain_works`.
- `--screenshot` captures the native window (or one dialog) with `PrintWindow` at 96 DPI, writes a PNG and exits 0. It uses sample data, not saved state, and starts no engine thread.
- `cargo run -- --unrar-test <folder-with-rar> <dest>` and `cargo run -- --par2-test <folder-with-par2>` exercise post-processing without the UI.
- `cargo run -- --rss-test` fetches configured feeds; `--rss-download-test` also downloads NZBs accepted by the filters.
- `powershell -ExecutionPolicy Bypass -File .\tray-verify.ps1` builds nothing; run `cargo build` first and close the debug NewsQueue. It runs in a temporary `NQ_CONFIG` folder and checks the tray icon, speed tooltip, balloon API result, minimize to tray (hidden, no taskbar button), continued engine tick (queue autosave), click-to-restore, close-to-tray and tray Exit.
- `.\build-installer.ps1` runs `package.ps1`, compiles `installer\newsqueue.iss` with Inno Setup 7 into `dist\NewsQueue-1.0.5-Setup.exe` and writes its SHA256.
- `.\installer-test.ps1` compiles a test variant of the installer (`NewsQueue Install Test`, own AppId) from `dist\NewsQueue-1.0.5` (run `package.ps1` first), installs it into a temp folder over a simulated IExpress install, checks files/registry/shortcuts/relaunch, launches the installed exe and uninstalls. It never touches a real NewsQueue install.
- `.\shutdown-verify.ps1` arms the safe `Exit` shutdown path in a debug build, hides the window in the tray and checks the 15-second countdown still exits the app (temporary `NQ_CONFIG`).
- `.\download-while-tray-verify.ps1` runs a local mock NNTP server, hides the app in the tray, and verifies a download still completes and the payload is correct. It works in a temporary `NQ_CONFIG` folder; `app/*.json` are not touched.
- Release builds use the Windows GUI subsystem, so they do not open a console. Use the debug build (`cargo run`) for CLI diagnostics and unit tests.
- `cargo fmt --check` currently fails on existing code. Run `cargo fmt` when formatting; don't claim the check passed unless you ran it.

## Where things live

- `main.rs` — native main window: thread-local `Ui` state, window procs, toolbars, tabs, queue/log/PAR2/RSS ListViews with custom draw, status bar, `refresh()` from the engine `Snapshot` on a 250 ms timer, engine events, shutdown countdown, Web access, screenshot demo. The engine runs on its own thread (`EngineHost`); the UI sends `Command`s.
- `queue.rs` — pure queue-tree logic (rows, cells, progress kinds, changed rows), unit-tested.
- `form.rs` — dialog framework and control helpers; `setup.rs` — the 15 Setup pages from the `FIELDS` table, server editor, scheduler grid; `pick.rs` — `IFileOpenDialog` pickers; `cli.rs` — the diagnostic CLI modes.
- `core/src/model.rs` — persisted types (`Settings`, `Collection`, `Server`), measured constants and formatters, NZB parser, sample data. New persisted fields need defaults (`#[serde(default)]`) so existing JSON still loads; add them to `setup.rs` and `SETTINGS_CHECKLIST.md`.
- `dialogs.rs` — stats, columns, pause-for, speed limit, shutdown, version/update, about, notes, rename, execute, unrar password, RSS feeds.
- `download.rs` — real NNTP/yEnc downloader. One active job at a time; all enabled servers are used and the global limit is split across workers. Resume metadata lives in `.nqpart` sidecars, article bodies are cached in memory (bounded by `article_cache_mb`), and connections support HTTP CONNECT/SOCKS5 proxies. Network code runs in std threads with a `Mutex<DownloadSnapshot>`; there is no async runtime.
- `container.rs` — payload sniffing (RAR5/volume number, RAR4, ZIP, 7Z, PAR2) so downloads whose names carry no extension are still saved and unpacked correctly.
- `par2.rs` — PAR2 verify/repair worker using `app/tools/par2.exe`; feeds the unrar stage.
- `rss.rs` — RSS/Atom fetch, filter matching and NZB autodownload using `ureq` with native TLS.
- `update.rs` — GitHub release check and verified silent self-update.
- `unrar.rs` — extraction worker for completed collections. Uses the vendored 64-bit `app/tools/UnRAR.exe` (the original's bundled DLL is 32-bit), detects password-protected archives for the UI prompt, and builds archive/group/NZB subfolders. Do not delete the tool.
- `src/classic.rs` — Windows 2000 colours for the desktop process (import-table redirect; never `SetSysColors`) and the repainted frames, captions, caption buttons, menu bar and scroll bars.
- `web/src/skin.rs`, `web/src/theme.rs` — the classic (Windows 2000) widgets, palette and drawn icons of the egui web UI.
- `tray.rs` — native Win32 `Shell_NotifyIconW` tray thread: hidden message window, Show/Exit menu, minimize/close-to-tray with `SW_HIDE`, speed tooltip, balloon hints and the shared exit flag. `--screenshot` never starts the tray.
- `shutdown.rs` — queue-drained power actions (`shutdown.exe /s`, `/h`, `SetSuspendState`, or app exit) with inspectable plans and a mock mode used by tests.
- `package.ps1` / `build-installer.ps1` — portable release zip and Inno Setup exe (`installer/newsqueue.iss`): per-user files, shortcuts, HKCU uninstall entry and `unins000.exe`; it replaces an older IExpress install.
- `installer-test.ps1` — end-to-end install/uninstall check against the built setup; uses temp paths and a clean state.
- `tray-verify.ps1` / `shutdown-verify.ps1` / `download-while-tray-verify.ps1` — scripted tray, shutdown-countdown and mock-NNTP download checks against the native window.
- `screenshots.ps1` — regenerates `shots/golden/` from the native window; `-Check` compares fresh captures with them (0.5 % tolerance).

## Gotchas

- The installer is per-user: `HKCU\...\Uninstall\{7A3E2C51-9B4D-4F0E-8C21-6D5B9E1A4F37}_is1`, Start Menu/Desktop shortcuts, `%LOCALAPPDATA%\Programs\NewsQueue` by default. Inno flags (`/VERYSILENT /DIR=`) or the old `NQ_SILENT`/`NQ_INSTALL_DIR`/`NQ_NO_DESKTOP` variables select a silent install.
- `app/dist` is generated release output (`package.ps1` / `build-installer.ps1`) and is ignored by git.
- Normal runs persist to `app/newsqueue-settings.json`, `app/newsqueue-queue.json`, `app/newsqueue-history.json` and `app/newsqueue-stats.json` (autosave every 30 s / `autosave_minutes`, and on exit). Don't hand-edit them while the app is running. Saved queue entries without segment IDs are discarded on load; sample/demo data appears only in screenshot mode.
- `app/download`, `app/log`, `app/nzb` and `app/temp` are runtime output, not source. `*.nqpart` files are resume sidecars; keep them until a download finishes.
- Test Usenet account, RSS API keys are configured only in the local, gitignored `app/newsqueue-settings.json`. Never copy them into source, docs, logs, commits or replies.
- `cargo run -- --download-test <file.nzb>` and Connect perform real network downloads into the download folder; don't run them unless asked. Use a local NZB such as `Civilization.IV-RELOADED.nzb` or `Love.Under.the.Mistletoe...nzb` (both ignored by git). Missing articles are skipped and mark the collection `Incomplete`; PAR2 repair and RAR extraction are real.
- For screenshots of specific UI, set `NQ_DEMO_DIALOG` (`setup`, `server`, `stats`, `notes`, `about`, `columns`, `pause`, `speed`, `version`, `shutdown`, `unrar-password`, `rename`, `execute`, `rss-feeds`), `NQ_DEMO_TAB` (`par2`, `log`, `rss`) or `NQ_DEMO_SETUP_PAGE=0..14`.
