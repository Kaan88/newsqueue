# CLAUDE.md

This file is the orientation guide for AI agents working on NewsQueue. Read it
before changing code. `AGENTS.md` contains the short day-to-day command list;
`ROADMAP.md` contains the current status and blockers; `app/MECHANICS.md` and
`app/mechanics.json` contain measured AltBinz behaviour.

## 1. What this software is

NewsQueue is a Windows-only, from-scratch Rust/egui clone of AltBinz 0.48.4. It
is a private-use desktop Usenet downloader, not a source-code or artwork copy of
the original. The parity target is AltBinz 0.48.4; NewsQueue itself uses its own
versioning.

- Current app version: **1.0.1**
- Current build date: **2026-11-04**
- Parity target: **AltBinz 0.48.4**
- Platform: Windows x64
- UI: eframe/egui 0.29, custom light theme
- Network: std threads, blocking sockets, no async runtime
- Out of scope: global/Newznab search (removed 2026-11-18; the toolbar has no
  Search button and Ctrl+F does nothing)

`v1.0.0` was deleted and recreated on 2026-11-04 because every download made
before the yEnc escape fix was corrupt; `v1.0.1` supersedes it with
content-based archive naming for obfuscated posts. Release assets keep their
build-output file names and carry no label.

## 2. Repository layout

Everything that is part of the application lives in `app/`.

| Path | Purpose |
|---|---|
| `app/src/main.rs` | `App` state, CLI entry, `App::tick()`, eframe `update`, timers, scheduler, worker start/finish, logging, autosave |
| `app/src/model.rs` | Persisted types (`Settings`, `Collection`, `Server`, `QueueFile`, `Segment`), measured constants, formatters, NZB parser, sample data |
| `app/src/ui.rs` | Main window, tabs, queue table, status bar, PAR2/RSS/Log tabs, queue actions |
| `app/src/dialogs.rs` | 15 setup pages, server editor, stats/columns, password prompt, shutdown, version/update and other modal windows |
| `app/src/download.rs` | Real NNTP/yEnc downloader: TLS/plain, HTTP CONNECT/SOCKS5, workers, cache, resume, retries |
| `app/src/container.rs` | Container detection from payload bytes (RAR5 volume numbers, RAR4, ZIP, 7Z, PAR2) used to name obfuscated downloads |
| `app/src/par2.rs` | PAR2 verify/repair worker using `app/tools/par2.exe` |
| `app/src/unrar.rs` | UnRAR extraction worker, options, archive-name/group/NZB subfolders, password detection |
| `app/src/rss.rs` | RSS/Atom fetch, filters, NZB autodownload |
| `app/src/update.rs` | GitHub release check, verified self-update via the silent installer |
| `app/src/theme.rs` | Measured colours, fonts, icon drawing, widget metrics |
| `app/src/tray.rs` | Win32 tray icon, off-screen hide/show, taskbar add/remove, tooltip, balloons |
| `app/src/shutdown.rs` | Queue-drained shutdown/hibernate/sleep/exit plan plus mock backend |
| `app/tools/` | Vendored `UnRAR.exe`, `par2.exe`, licenses; do not delete |
| `app/tests/fixtures/` | `tiny.rar`, `single.rar`, `encrypted.rar` fixtures |
| `app/shots/` | Current screenshot and golden tab/dialog screenshots |
| `app/installer/` | IExpress payload scripts: `install.ps1`, `uninstall.ps1`, `install.cmd` |
| `app/package.ps1` | Builds the portable release folder/zip |
| `app/build-installer.ps1` | Builds `NewsQueue-<version>-Setup.exe` with IExpress |
| `app/installer-test.ps1` | End-to-end install/uninstall test |
| `app/tray-verify.ps1` | Scripted tray/off-screen/taskbar/tooltip/balloon check |
| `app/shutdown-verify.ps1` | Scripted off-screen shutdown-countdown check |
| `app/download-while-tray-verify.ps1` | Mock-NNTP download while off-screen |
| `app/screenshots.ps1` | Regenerates golden screenshots |

Read-only evidence directories: `altbinz_original/`, `capture/`,
`investigation.cany/` and `reference/`. Do not edit them to make a test pass.
Harness files such as `craft.json`, `feed.json`, `build-activity.log` and
`checks/` are generated; do not edit them.

## 3. Runtime data and privacy

Normal runs persist to `app/`:

- `newsqueue-settings.json`
- `newsqueue-queue.json`
- `newsqueue-history.json`
- `newsqueue-stats.json`
- `log/`
- `download/`, `nzb/`, `temp/`, `unrar/`

All are gitignored. They can contain the real Usenet account and API keys. Never
copy credentials or feed/index URLs into source, docs, logs, commits or replies.

`app/download`, `app/log`, `app/nzb` and `app/temp` are runtime output, not source.
`*.nqpart` files are resume sidecars. Keep them until a download finishes.

Sample/demo data is used only by `--screenshot` mode. Normal runs must not show
sample queue entries, fake statistics or seeded logs.

`app_base()` returns the project `app/` directory for debug/release binaries
launched from `target/debug` or `target/release`, otherwise the executable's
directory. This is why the installed app keeps its settings beside the exe.

## 4. Main architecture and data flow

### Application loop

`App::new` loads settings, queue, history and stats, creates the eframe state and
starts optional startup behaviour. `App::update` runs every frame:

1. `handle_tray`
2. `tick`
3. handle screenshot/close state
4. draw UI and dialogs

`App::tick` handles all timers, scheduler, queue start, worker polling, speed,
statistics, autosaves, RSS timer, auto-import, shutdown countdown and execute
hooks. If `tick` does not run, downloads, tray speed and shutdown freeze.

### Downloader

- One active download job at a time (`active_dl`).
- A job opens the configured number of connections for each enabled server.
- Workers share a thread-safe task queue and update a `Mutex<DownloadSnapshot>`.
- The UI polls the snapshot; there is no async runtime.
- Resume state is stored in `<file>.nqpart` sidecars listing completed article IDs.
- Missing `430` articles increment `missing`, do not kill the job, and mark the
  collection `Incomplete` so PAR2 can repair it.
- `Stage::Incomplete` is distinct from `Error` and `Completed`.
- Article bodies are cached in memory with a byte limit (`article_cache_mb`) and
  cleanup interval.
- Connections support HTTP CONNECT and SOCKS5 proxies; TLS is negotiated after
  the proxy tunnel.
- Retry/backoff and per-thread priority are configured in settings.

### PAR2 → unrar chain

`schedule_after_download` starts PAR2 when configured and present, otherwise
unrar. `poll_par2` and `poll_unrar` are called from `tick`. Successful PAR2 starts
unrar; successful unrar triggers the per-collection/default execute hook.

The PAR2 and unrar workers use `std::process::Command` and channels. Tools are
vendored; do not replace them with the original 32-bit DLLs.

### Unrar

`UnrarOptions` carries overwrite/rename, strip-root, delete-archive,
delete-empty-folder, auto-rename, collection name, password, move/delete
patterns and expected files.

`target_dir` builds the destination in this order:

1. collection unrar folder, or configured unrar folder, or collection folder
2. archive-name subfolder (`create_dir_archive_name`)
3. group subfolder (`create_dir_group_name`)
4. NZB-name subfolder (`create_dir_nzb_name`)

Encrypted archives are detected from UnRAR output. `UnrarError::PasswordRequired`
puts the app into the password-prompt state (`show_unrar_password`,
`unrar_password_target`, `unrar_password_draft`). Submitting a password stores it
on the collection and retries extraction. Cancelling marks the collection `Error`.
Do not turn this into a blocking modal call from the worker thread.

### Tray and off-screen behaviour

eframe stops calling `tick` when the window is hidden or minimized. NewsQueue
therefore keeps the window **visible off-screen** and removes its taskbar entry
with `ITaskbarList3::DeleteTab`; restoring calls `AddTab` and restores the saved
`WINDOWPLACEMENT`. Do not replace this with `ShowWindow(SW_HIDE)` or normal
minimize, or downloads/speed/shutdown will freeze.

The tray thread owns the icon and message loop. It uses:
- `Shell_NotifyIconW` for the icon, tooltip and balloons.
- `WM_TIMER` for the speed tooltip.
- COM `ITaskbarList3` for taskbar add/remove.
- A shared exit flag for tray Exit.

`NQ_TRAY_TIP_FILE`, `NQ_TRAY_BALLOON_FILE`, `NQ_TRAY_FORCE_SPEED`,
`NQ_TRAY_TEST_SPEED_BPS`, `NQ_TRAY_FORCE_BALLOON` and `NQ_TRAY_DEBUG` are
diagnostic/test hooks used by `tray-verify.ps1`. They have no effect when unset.

### Shutdown

`shutdown.rs` exposes `PowerAction::{Shutdown,Hibernate,Sleep,ExitApplication}`,
inspectable `Plan` values and `ShutdownMode::{Real,Mock}`. Tests use `Mock` so no
real power command runs. The UI countdown is 15 seconds and can be aborted.
`NQ_TEST_ARM_SHUTDOWN=exit` is a debug-only test hook used by
`shutdown-verify.ps1`.

## 5. Settings structure

The Setup window has 15 pages in the original order (NZB Play and User
interface #2 were removed, see `REMOVED.md`):

Download, Download #2, NZB, NZB #2, Servers, Log, Connection, Proxy, Scheduler,
Par2, Unpack, Miscellaneous, Miscellaneous #2, Miscellaneous #3,
User interface.

Persisted fields live in `model.rs` `Settings`. New fields need
`#[serde(default)]` support so existing JSON still loads. Keep the UI checkbox
and the actual behaviour wired together.

Important settings areas:

- Connection/servers, TLS/plain, connections, retries, proxy
- Download folders, skip-existing, auto-import, delete NZB after import
- NZB naming
- PAR2 checking/repairing and tool options
- Unrar/unpack options, archive/group/NZB subfolders, password per collection
- Scheduler, speed limits, startup connect
- Log/history/statistics retention and real versus demo data
- Tray: minimize to tray, close to tray, balloon hints, speed in tray
- Shutdown method
- UI columns, tabs, fonts/theme-adjacent flags

## 6. Build, run and test

Run from `app/`:

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

Single tests:

```powershell
cargo test speed_limit_uses_decompiled_formula
cargo test unrar
cargo test par2
cargo test par2_repair_then_unrar_chain_works
cargo test unrar_password_prompt
cargo test shutdown
cargo test tray
```

Diagnostics:

```powershell
cargo run -- --unrar-test <folder-with-rar> <dest>
cargo run -- --par2-test <folder-with-par2>
cargo run -- --rss-test
cargo run -- --rss-download-test
cargo run -- --download-test <file.nzb>
.\tray-verify.ps1
.\shutdown-verify.ps1
.\download-while-tray-verify.ps1
.\installer-test.ps1
.\screenshots.ps1
```

Release builds use the Windows GUI subsystem (`#![cfg_attr(not(debug_assertions),
windows_subsystem = "windows")]`), so they do not open a console window. Use the
debug build for CLI diagnostics and test output.

`cargo fmt --check` currently fails on existing code. Run `cargo fmt` only when
you intend to format; do not claim the check passed unless it was run.

## 7. Tests and verification

- 64 unit/integration tests currently pass.
- `download.rs` has a mock NNTP server used for success, missing-article,
  article-cache, 430/CRC server failover, reconnect-on-retry and duplicate-name
  tests.
- `unrar.rs` has fixtures for plain, single-file and encrypted RAR archives.
- `par2.rs` generates damaged files and a damaged RAR, repairs with `par2.exe`
  and extracts with `UnRAR.exe`.
- `main.rs` integration tests cover settings, execute hooks, auto-import, shutdown mock behaviour and the unrar password prompt/retry.
- `tray-verify.ps1` checks the tray icon rect, tooltip speed, balloon API result,
  off-screen minimize, taskbar removal, continued tick/autosave, restore,
  close-to-tray and tray exit.
- `shutdown-verify.ps1` checks the 15-second countdown exits while off-screen.
- `download-while-tray-verify.ps1` runs a local mock NNTP server and verifies a
  download completes while the window is off-screen.
- `installer-test.ps1` installs the setup into a temp folder, checks files,
  registry, shortcuts and installed-exe launch, then uninstalls.
- `screenshots.ps1` regenerates `shots/golden/` (queue, tabs, every major dialog,
  shutdown and unrar password prompt).

## 8. Versioning, packaging and releases

NewsQueue uses independent versions. Current version is **1.0.1**.
`v1.0.0` was rebuilt in place once (yEnc decode fix) and then superseded by
`v1.0.1`, which adds content-based naming for obfuscated posts. There is no
`Version?` and a timer (`check_version_minutes`, 0 = never) check GitHub
releases; with `auto_update` the app installs a newer setup silently when idle.
Only releases built with the current `install.ps1` (NQ_WAIT_PID/NQ_RELAUNCH)
can self-update. The repo must be publicly readable for the check to work.

Portable package:

```powershell
.\package.ps1
```

Installer:

```powershell
.\build-installer.ps1
.\installer-test.ps1
```

The IExpress installer is per-user:

- installs to `%LOCALAPPDATA%\Programs\NewsQueue`
- creates Start Menu and Desktop shortcuts
- writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue`
- includes an uninstaller
- keeps existing settings on reinstall
- `NQ_INSTALL_DIR`, `NQ_SILENT`, `NQ_NO_DESKTOP` are test overrides
- IExpress cannot stage from a path with spaces; `build-installer.ps1` uses
  `%TEMP%`

`app/dist` is generated output and gitignored. Do not commit it unless the user
asks for a release artifact.

When releasing, update all version references (`Cargo.toml`, `model.rs`,
user agents, scripts, installer, README, ROADMAP, AGENTS), rebuild the portable
zip and installer, run `installer-test.ps1`, and attach the setup, checksum and
zip to the GitHub release. Never include settings, API keys or downloaded NZBs.

Upload the built files exactly as named on disk (`NewsQueue-<version>.zip`,
`NewsQueue-<version>-Setup.exe`, `NewsQueue-<version>-Setup.exe.sha256`) with no
`#label` on the asset. Do not rename artifacts on upload.

## 9. Visual/UI conventions

- Measured identity: Microsoft Sans Serif-like UI at roughly 11 px (8.25 pt),
  main toolbar 55 px high, toolbar buttons 55×53, tab strip ~26 px, status bar
  21 px, queue secondary toolbar 30 px.
- Main window default client is 1285×881; original design is 1490×668.
- Queue columns default from `QUEUE_COLUMN_WIDTHS`; the saved AltBinz config may
  use a wider subject column.
- Queue rows use dense 19 px rows, no alternating stripes, gray completed
  progress bars, 10–11 px text.
- Setup pages use the original order and a roughly 700×580 window.
- Queue rows include a transparent `row_click_overlay`; preserve it
  when editing cells or labels.
- Keep icons/theme drawn, not copied from the original artwork.

## 10. Known gotchas

- Do not hide or minimize the main window normally: eframe stops ticking. Use the
  off-screen + taskbar-removal path in `tray.rs`.
- Do not add an async runtime. The network and worker code is deliberately
  blocking std threads with channels and mutexes.
- Do not truncate partial downloads without checking `.nqpart` metadata.
- yEnc escapes twice: a byte is stored as `(b + 42)` and, when that value is
  NUL/LF/CR/`=`, additionally as `'='` plus `(value + 64)`. Decoding an escaped
  byte therefore subtracts 106, not 64. `decode_yenc` in `download.rs` had this
  wrong until 2026-11-04, which silently corrupted every byte whose encoded form
  needed escaping (~1.6% of a random payload, invisible to length checks). Keep
  `decodes_yenc_escapes` and `downloads_binary_articles_with_yenc_escapes`
  passing; the latter round-trips all 256 byte values through a mock server.
- An article that a server answers 430 for, or whose yEnc `pcrc32` does not
  match, is handed to workers of servers that have not tried it; it counts as
  missing only when no living server can try it. Failed connections are
  replaced before a retry. Files of one job never share a path (`unique_name`).
- Execute hooks get `{folder}`/`{name}`/`{file}` through `NQ_FOLDER`/`NQ_NAME`/
  `NQ_FILE` environment variables inside quotes; never paste them into the
  shell line as text.
- Missing articles are not fatal; they must mark the collection `Incomplete` and
  allow PAR2 repair.
- Obfuscated posts can carry random names with no extension. Never trust the
  name alone: `container::sniff` decides the extension, and RAR5 volumes are
  renamed `<collection>.partNNN.rar` from the volume number in their header so
  UnRAR can chain them. `unrar::normalize_obfuscated_names` repairs folders that
  were downloaded before this existed.
- Saved queue entries without segment IDs are discarded on load.
- `UnRAR.exe`/`par2.exe` are vendored and required. The original DLLs are 32-bit
  and cannot be loaded by this 64-bit build.
- PAR2/Unrar process output must be parsed carefully: a path containing the word
  "password" is not a password error. Use the specific messages
  ("incorrect password", "enter password", etc.).
- Trigger a password retry only through the UI state; never prompt from the worker
  thread.
- Release builds have no console window. Debug builds do.
- `app/newsqueue-settings.json` and other runtime JSON files are local-only.
- Self-update never runs from dev builds under `target\`; they only show a notice.
  `NQ_UPDATE_REPO=owner/name` overrides the repo for testing.
- The original binaries/evidence are read-only. Use Ghidra/DFM evidence rather
  than guessing measured constants.
- Keep `MECHANICS.md` and `mechanics.json` in sync when a measured value changes.

## 11. Current roadmap state and blockers

Done: downloader, PAR2 core chain, unrar options and password prompt, RSS,
real statistics/history, version check/self-update,
tray, off-screen behaviour, shutdown, portable packaging and installer.

Not done / pending:

- Optional code signing, shortcut polish and a per-setting checklist.

Blocked on user-supplied data:

- A currently available release with a valid PAR2 set for live repair validation.
- A second Usenet server account for live failover.

Live check on 2026-11-18 (single server, after the CRC and failover changes):
`Love.Under.the.Mistletoe` (1.21 GB, 5 files) downloaded with 0 missing articles
and PAR2 verified it clean. `Civilization.IV` (1.63 GB, 40 files, `.rar`/`.rNN`)
downloaded with 0 missing articles; PAR2 reported damage and repaired it, then
UnRAR extracted `rld-civ4.iso` (1.45 GB) without errors. The "repair" was not
damage: a verify-only run on a fresh download shows all 2270 RAR/SFV data
blocks intact and only `reloaded.nfo` missing. The NZB's 40 files do not include
that NFO, but the PAR2 set covers it, so PAR2 rebuilt it from one recovery
block. The downloader output was byte-exact.

## 12. Where to start for common changes

- New setting: `model.rs` field + default, `dialogs.rs` checkbox/field, actual
  behaviour in `tick`/worker, test.
- New queue action: `UiAction`, `ui.rs` menu/button,
  `main.rs` handler, test.
- New network feature: `download.rs`, keep the snapshot/channel pattern.
- New post-processing: `unrar.rs`/`par2.rs`, keep `UnrarError` classification,
  add a fixture/test.
- New tray behaviour: `tray.rs`, remember off-screen visible + taskbar removal.
- New shutdown action: `shutdown.rs` plus App countdown/state tests.
- New setup page: `SETUP_PAGES` and the page match in `dialogs.rs`; preserve the
  original order.
- Screenshot/UI change: update `theme.rs`/`ui.rs`, run `screenshots.ps1`, inspect
  `shots/golden/` for regressions.

Before finishing any task: run the relevant focused test(s), then `cargo test`,
then `cargo run -- --screenshot shots/now.png`. For tray/installer/post-processing
changes, also run the corresponding script. Report exact evidence rather than
claiming success.
