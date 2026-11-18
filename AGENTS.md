# AGENTS.md

## Project

- Read `CLAUDE.md` for the full architecture, verification and release guide, and `ROADMAP.md` for current work and blockers.
- `app/` is the whole Rust 2021 project (eframe/egui 0.37): a from-scratch clone of AltBinz, presented as NewsQueue 1.0.5. The parity target is still AltBinz 0.48.4.
- `app/MECHANICS.md` records measured constants, formulas and function addresses from the original Delphi binary; `app/mechanics.json` is the machine-checked list. Keep both in sync with `model.rs`, and only mark a value "estimated" after searching the evidence.
- `capture/`, `investigation.cany/`, `altbinz_original/` and `reference/` are read-only evidence. Harness files (`craft.json`, `feed.json`, `build-activity.log`, `checks/`) are generated; don't edit them. This directory is not a git repo.
- Ghidra 12.1.4 is installed at `C:\Users\kaank\AppData\Roaming\craft-anything\engines\ghidra\ghidra_12.1.4_PUBLIC`. Use it when the recorded evidence is insufficient to verify a value or behavior; do not guess mechanics.

## Commands (from `app/`)

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

- Single test: `cargo test speed_limit_uses_decompiled_formula`; also `cargo test unrar`, `cargo test par2`, and `cargo test par2_repair_then_unrar_chain_works`.
- `--screenshot` draws about 120 frames, writes a PNG and exits 0. It uses default settings and the sample queue, not saved state.
- `cargo run -- --unrar-test <folder-with-rar> <dest>` and `cargo run -- --par2-test <folder-with-par2>` exercise post-processing without the UI.
- `cargo run -- --rss-test` fetches configured feeds; `--rss-download-test` also downloads NZBs accepted by the filters.
- `powershell -ExecutionPolicy Bypass -File .\tray-verify.ps1` builds nothing; run `cargo build` first and close any running NewsQueue. It checks the tray icon, speed tooltip, balloon API result, off-screen minimize, taskbar removal, continued tick/autosave, click-to-restore, close-to-tray and tray Exit.
- `.\build-installer.ps1` runs `package.ps1`, stages in `%TEMP%`, builds `dist\NewsQueue-1.0.5-Setup.exe` with IExpress and writes its SHA256.
- `.\installer-test.ps1` installs the built setup into a temp folder, checks files/registry/shortcuts, launches the installed exe and uninstalls. It refuses to run if any NewsQueue install, shortcut or process exists.
- `.\shutdown-verify.ps1` arms the safe `Exit` shutdown path in a debug build, minimizes to the tray and checks the 15-second countdown still exits the app.
- `.\download-while-tray-verify.ps1` runs a local mock NNTP server, minimizes the app, and verifies a download still completes while off-screen and the payload is correct. It backs up and restores the local settings/queue/history/stats files.
- Release builds use the Windows GUI subsystem, so they do not open a console. Use the debug build (`cargo run`) for CLI diagnostics and unit tests.
- `cargo fmt --check` currently fails on existing code. Run `cargo fmt` when formatting; don't claim the check passed unless you ran it.

## Where things live

- `main.rs` — `App` state, CLI entry, key handling, and `App::tick()`: all timers, scheduler, free-space pause, worker start/finish, logging and autosaves flow through here.
- `model.rs` — persisted types (`Settings`, `Collection`, `Server`), measured constants and formatters, NZB parser, sample data. New persisted fields need defaults (`#[serde(default)]`) so existing JSON still loads.
- `ui.rs` — main window, tabs, queue table, status bar. Queue rows add a transparent `row_click_overlay` so clicks on labels and progress bars reach the row; preserve it when editing columns.
- `dialogs.rs` — 15 setup pages, server editor, stats/columns and other modal windows.
- `download.rs` — real NNTP/yEnc downloader. One active job at a time; all enabled servers are used and the global limit is split across workers. Resume metadata lives in `.nqpart` sidecars, article bodies are cached in memory (bounded by `article_cache_mb`), and connections support HTTP CONNECT/SOCKS5 proxies. Network code runs in std threads with a `Mutex<DownloadSnapshot>`; there is no async runtime.
- `container.rs` — payload sniffing (RAR5/volume number, RAR4, ZIP, 7Z, PAR2) so downloads whose names carry no extension are still saved and unpacked correctly.
- `par2.rs` — PAR2 verify/repair worker using `app/tools/par2.exe`; feeds the unrar stage.
- `rss.rs` — RSS/Atom fetch, filter matching and NZB autodownload using `ureq` with native TLS.
- `update.rs` — GitHub release check and verified silent self-update.
- `unrar.rs` — extraction worker for completed collections. Uses the vendored 64-bit `app/tools/UnRAR.exe` (the original's bundled DLL is 32-bit), detects password-protected archives for the UI prompt, and builds archive/group/NZB subfolders. Do not delete the tool.
- `theme.rs` — measured colors, fonts, icons and widget metrics.
- `tray.rs` — native Win32 `Shell_NotifyIconW` tray thread: hidden message window, Show/Exit menu, off-screen minimize/close-to-tray with the taskbar entry removed, speed tooltip, balloon hints and the shared exit flag. `main.rs` cancels window close unless the tray or shutdown requested exit. `--screenshot` never starts the tray.
- `shutdown.rs` — queue-drained power actions (`shutdown.exe /s`, `/h`, `SetSuspendState`, or app exit) with inspectable plans and a mock mode used by tests.
- `package.ps1` / `build-installer.ps1` — portable release zip and IExpress setup exe. `installer/install.ps1` copies the payload, creates shortcuts and the HKCU uninstall entry; `installer/uninstall.ps1` removes them and schedules deletion of the install folder.
- `installer-test.ps1` — end-to-end install/uninstall check against the built setup; uses temp paths and a clean state.
- `tray-verify.ps1` / `shutdown-verify.ps1` / `download-while-tray-verify.ps1` — scripted off-screen tray, shutdown-countdown and mock-NNTP download checks.

## Gotchas

- The installer is per-user: it writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue`, Start Menu/Desktop shortcuts and `%LOCALAPPDATA%\Programs\NewsQueue` by default. `NQ_INSTALL_DIR`, `NQ_SILENT` and `NQ_NO_DESKTOP` override these for testing. IExpress cannot build from a path containing spaces, so `build-installer.ps1` stages under `%TEMP%`.
- `app/dist` is generated release output (`package.ps1` / `build-installer.ps1`) and is ignored by git.
- Normal runs persist to `app/newsqueue-settings.json`, `app/newsqueue-queue.json`, `app/newsqueue-history.json` and `app/newsqueue-stats.json` (autosave every 30 s / `autosave_minutes`, and on exit). Don't hand-edit them while the app is running. Saved queue entries without segment IDs are discarded on load; sample/demo data appears only in screenshot mode.
- `app/download`, `app/log`, `app/nzb` and `app/temp` are runtime output, not source. `*.nqpart` files are resume sidecars; keep them until a download finishes.
- Test Usenet account, RSS API keys are configured only in the local, gitignored `app/newsqueue-settings.json`. Never copy them into source, docs, logs, commits or replies.
- `cargo run -- --download-test <file.nzb>` and Connect perform real network downloads into the download folder; don't run them unless asked. Use a local NZB such as `Civilization.IV-RELOADED.nzb` or `Love.Under.the.Mistletoe...nzb` (both ignored by git). Missing articles are skipped and mark the collection `Incomplete`; PAR2 repair and RAR extraction are real.
- For screenshots of specific UI, set `NQ_DEMO_DIALOG` (`setup`, `server`, `stats`, `notes`, `about`, `columns`, `pause`, `shutdown`, `unrar-password`), `NQ_DEMO_TAB` (`par2`, `explorer`, `log`, `rss`), `NQ_DEMO_SETUP_PAGE=0..16`, `NQ_DEMO_EXPLORER=<folder>`, or `NQ_DEMO_CONNECT=1`.
