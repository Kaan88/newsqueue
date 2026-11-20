# CLAUDE.md

This file is the orientation guide for AI agents working on NewsQueue. Read it
before changing code. `AGENTS.md` contains the short day-to-day command list;
`ROADMAP.md` contains the current status and blockers; `app/MECHANICS.md` and
`app/mechanics.json` contain measured AltBinz behaviour.

## 1. What this software is

NewsQueue is a private-use Usenet downloader with a shared Rust engine, native
Win32 desktop, Rust server/Docker deployment and egui/WASM web UI. It began as
a from-scratch AltBinz 0.48.4 clone, not a source-code or artwork copy.
**AltBinz parity is no longer a requirement.** Measurements remain reference
evidence and must not be invented or silently changed.

Product direction (2026-11-20): reliability before appearance. The desktop NZB
picker failure is fixed locally and verified with isolated imports; keep its regression checks passing before release or a broad restyle.
The user approved the polished native Compact mockup in `app/native-mockups`.
The production desktop now uses its common-controls v6 styling and OS-managed
chrome, with existing commands and saved dimensions retained. The web UI must remain egui and retain its
layout, with Caustica as default and Classic selectable, separately from
System/Light/Dark brightness. Release 1.0.9 ships matching Windows, server and web builds.
See `ROADMAP.md` for active blockers and `CONTINUATION_PLAN.md` for gates.

- Current app version: **1.0.9**
- Current build date: **2026-11-20**
- Historical reference: **AltBinz 0.48.4**
- Platforms: Windows x64 desktop; headless server on Windows/Linux
- UI: native Win32 (`windows` 0.61), light only; web UI egui 0.37 (WASM)
- Network: std threads, blocking sockets, no async runtime
- Out of scope: global/Newznab search (removed 2026-11-18; the toolbar has no
  Search button and Ctrl+F does nothing)

`v1.0.0` was deleted and recreated on 2026-11-04 because every download made
before the yEnc escape fix was corrupt; `v1.0.1` supersedes it with
content-based archive naming for obfuscated posts. Release assets keep their
build-output file names and carry no label.

## 2. Repository layout

Everything that is part of the application lives in `app/`. It is a Cargo
workspace: `app/` is the desktop package (`newsqueue`, native Win32), `app/core`
is `newsqueue-core` (engine, no UI/Win32; modules `model`, `container`,
`download`, `unrar`, `par2`, `rss`), `app/server` is `newsqueue-server` (JSON
API: a library used by the desktop's Web access plus the headless binary) and
`app/web` is `newsqueue-web` (egui web UI, `wasm32-unknown-unknown`, built by
`web/build-web.ps1` into `web/dist`). Plain `cargo test` in `app/` runs the
first three (`default-members`). `NQ_CONFIG` overrides `app_base()`; always pass
`--config` to a dev server, or it shares the desktop's `app/` files.

| Path | Purpose |
|---|---|
| `app/src/main.rs` | Native main window: `Ui` state (thread-local), window procs, toolbars, tabs, queue/log/PAR2/RSS ListViews, status bar, custom draw, `refresh()` from the `Snapshot`, engine events, shutdown countdown, Web access, `--screenshot` demo |
| `app/src/queue.rs` | Pure queue-tree logic: rows (collections + expanded files), cell texts, progress bar kinds, changed-row detection (tested) |
| `app/src/form.rs` | Dialog framework (`create`, `run_modal`, `show_modeless`, `Ev` handler closures) and control helpers (label/edit/spin/check/combo/listview...) |
| `app/src/setup.rs` | Modeless Setup: 15 pages from the `FIELDS` table bound to `Settings` JSON, server list + editor, scheduler grid |
| `app/src/pick.rs` | File/folder pickers (`IFileOpenDialog`/`IFileSaveDialog`) |
| `app/src/cli.rs` | `--download-test`, `--par2-test`, `--unrar-test`, `--rss-test`, `--rss-download-test`, `--update-test` |
| `app/src/classic.rs` | Legacy styling evidence, test-only; no import-table patches or caption emulation in production |
| `app/src/util.rs` | DPI scaling, fonts, window creation, icons/image lists, `PrintWindow` capture for screenshots |
| `app/core/src/engine.rs` | `Engine`: queue, settings, history, stats, logs, speed/scheduler timers, download/PAR2/unrar/RSS workers, auto-import, execute hooks, `Engine::tick`; reports to the UI through `Event`s (`take_events`) |
| `app/core/src/command.rs` | `Command` enum + `Engine::apply`: id-addressed queue edits for every front end |
| `app/core/src/snapshot.rs` | `Snapshot` (secret-free queue/speed/log view) and `EngineHandle` (send `Command`, read snapshot) for other front ends |
| `app/server/` | `newsqueue-server`: headless engine + JSON API (`tiny_http`), GUI_PLAN Phase 1 slice; `--config`/`NQ_CONFIG`, `--api-key`/`NQ_API_KEY` |
| `app/core/src/host.rs` | `EngineHost`: engine timer thread; the UI `checkout()`s it per frame and `checkin()`s after |
| `app/core/src/model.rs` | Persisted types (`Settings`, `Collection`, `Server`, `QueueFile`, `Segment`), measured constants, formatters, NZB parser, sample data |
| `app/src/dialogs.rs` | Stats (charts + history), columns, pause-for, speed limit, shutdown, version/update, about, notes, rename, execute, unrar password, RSS feeds; `demo()` for screenshots |
| `app/core/src/download.rs` | Real NNTP/yEnc downloader: TLS/plain, HTTP CONNECT/SOCKS5, workers, cache, resume, retries |
| `app/core/src/container.rs` | Container detection from payload bytes (RAR5 volume numbers, RAR4, ZIP, 7Z, PAR2) used to name obfuscated downloads |
| `app/core/src/par2.rs` | PAR2 verify/repair worker using `app/tools/par2.exe` |
| `app/core/src/unrar.rs` | UnRAR extraction worker, options, archive-name/group/NZB subfolders, password detection |
| `app/core/src/rss.rs` | RSS/Atom fetch, filters, NZB autodownload |
| `app/src/update.rs` | GitHub release check, verified self-update via the silent installer |
| `app/web/src/skin.rs`, `theme.rs` | Classic (Windows 2000) widgets, palette and drawn icons of the egui web UI (light/dark) |
| `app/src/tray.rs` | Win32 tray icon, `SW_HIDE`/show of the main window, tooltip, balloons |
| `app/src/shutdown.rs` | Queue-drained shutdown/hibernate/sleep/exit plan plus mock backend |
| `app/tools/` | Vendored `UnRAR.exe`, `par2.exe`, licenses; do not delete |
| `app/tests/fixtures/` | `tiny.rar`, `single.rar`, `encrypted.rar` fixtures |
| `app/shots/` | Current screenshot and golden tab/dialog screenshots |
| `app/installer/` | `newsqueue.iss`, the Inno Setup 7 script |
| `app/package.ps1` | Builds the portable release folder/zip |
| `app/build-installer.ps1` | Builds `NewsQueue-<version>-Setup.exe` with Inno Setup 7 (`ISCC.exe`, or `NQ_ISCC`) |
| `app/installer-test.ps1` | End-to-end install/uninstall test |
| `app/tray-verify.ps1` | Scripted tray/hide/taskbar/tooltip/balloon check (temp `NQ_CONFIG`) |
| `app/shutdown-verify.ps1` | Scripted shutdown-countdown check while hidden in the tray |
| `app/download-while-tray-verify.ps1` | Mock-NNTP download while hidden in the tray |
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
- `newsqueue-picker-folders.json` (last successful open folder per picker)
- `log/`
- `download/`, `nzb/`, `temp/`, `unrar/`

All are gitignored. They can contain the real Usenet account and API keys. Never
copy credentials or feed/index URLs into source, docs, logs, commits or replies.

`app/download`, `app/log`, `app/nzb` and `app/temp` are runtime output, not source.
`*.nqpart` files are resume sidecars. Keep them until a download finishes.

Sample/demo data is used only by `--screenshot` and `--ui-smoke` modes. Normal runs must not show
sample queue entries, fake statistics or seeded logs.

`app_base()` returns the project `app/` directory for debug/release binaries
launched from `target/debug` or `target/release`, otherwise the executable's
directory. This is why the installed app keeps its settings beside the exe.

## 4. Main architecture and data flow

### Application loop

The engine lives in `newsqueue-core` (`engine.rs`) and runs on its own thread
(`EngineHost::start`), so it keeps ticking whatever the window does. The
desktop UI reads the secret-free `Snapshot` through its `EngineHandle` and
sends `Command`s (`send_cmd`); `with_engine(|e| ...)` checks the engine out
for the few cases that need it (Setup needs the full settings with secrets,
logging). Engine code must not touch UI state: queue an `Event` instead;
`main.rs` drains them in `tick()` (toast, message box, tray balloon, unrar
password, RSS feeds needed).

`main()` runs CLI diagnostics (`cli.rs`), creates the window and runs a plain
`GetMessageW` loop (`pre_translate` for shortcuts, `form::is_dialog_message`
for modeless dialogs). A 250 ms `WM_TIMER` calls `refresh()`, which redraws only
the rows whose content changed (`queue::signature`/`changed_ranges`), updates
the toolbar, status bar and tray, and runs the shutdown countdown and update
check. UI state lives in a thread-local `RefCell<Ui>` reached through short
`ui(|u| ...)` borrows: never send a message to a control inside a borrow (its
window proc may re-enter `ui`). Screenshot mode starts no engine thread; it
pumps the sample engine with `EngineHost::pump_local` (no tick, so the sample
stays as it is).

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

### Tray

The engine runs on its own thread, so the window can be hidden normally:
minimize-to-tray and close-to-tray call `ShowWindow(SW_HIDE)` (which also drops
the taskbar button) and the tray restores with `SW_SHOW`/`SW_RESTORE`. The tray
thread owns the icon and message loop and finds the main window by its exact
title (`NewsQueue 1.0.9`). It uses:
- `Shell_NotifyIconW` for the icon, tooltip and balloons.
- `WM_TIMER` for the speed tooltip.
- A shared exit flag for tray Exit (the main window then closes normally).

`NQ_TRAY_TIP_FILE`, `NQ_TRAY_BALLOON_FILE`, `NQ_TRAY_FORCE_SPEED`,
`NQ_TRAY_TEST_SPEED_BPS`, `NQ_TRAY_FORCE_BALLOON` and `NQ_TRAY_DEBUG` are
diagnostic/test hooks used by `tray-verify.ps1`. They have no effect when unset.

### Web access

Setup -> Miscellaneous #3 (`web_access`, `web_access_bind`, `web_access_key`,
all private settings) starts `newsqueue_server::spawn` inside the desktop app
on the same `EngineHandle`, serving the JSON API and the egui web UI (`web`
beside the exe, packaged from `web/dist` when it has been built). A
non-loopback address needs a password. Turning it off takes effect after a
restart.

### Shutdown

`shutdown.rs` exposes `PowerAction::{Shutdown,Hibernate,Sleep,ExitApplication}`,
inspectable `Plan` values, `ShutdownMode::{Real,Mock}` and the testable
`Countdown` (starts when everything is done, cancels, fires once). Tests use
`Mock` so no real power command runs. The UI countdown is 15 seconds and can be aborted.
`NQ_TEST_ARM_SHUTDOWN=exit` is a debug-only test hook used by
`shutdown-verify.ps1`.

## 5. Settings structure

The Setup window (`setup.rs`) has 15 pages in the original order (NZB Play and
User interface #2 were removed, see `REMOVED.md`). Plain fields are rows of
the `FIELDS` table (page, settings key, label, kind); Servers and Scheduler are
hand-made pages:

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

Keep test launches hidden/off-screen without desktop focus or control, per the
user preference in `AGENTS.md`. Run interactive checks only when explicitly
requested. `app/PICKER_SMOKE.md` records the completed picker interaction run.

- Historical verification (2026-11-18): 113 unit/integration tests passed (78 in core, 23 in desktop, 12 in server);
  `cargo test -p newsqueue-web` runs the web UI's egui test natively (1).
- `SETTINGS_CHECKLIST.md` lists every `Settings` field, its Setup page and
  the code that reads it; regenerate/extend it when adding a setting.
- `download.rs` has a mock NNTP server used for success, missing-article,
  article-cache, 430/CRC server failover, reconnect-on-retry and duplicate-name
  tests.
- `unrar.rs` has fixtures for plain, single-file and encrypted RAR archives.
- `par2.rs` generates damaged files and a damaged RAR, repairs with `par2.exe`
  and extracts with `UnRAR.exe`.
- Desktop tests cover the queue tree (`queue.rs`), the Setup binding table
  (every key exists, none twice), the shutdown countdown, tray and update
  logic. Engine behaviour (settings, execute hooks, auto-import, unrar
  password retry) is tested in `core`.
- `tray-verify.ps1` checks the tray icon rect, tooltip speed, balloon API result,
  minimize to tray (hidden, no taskbar button), continued engine tick (queue
  autosave), restore, close-to-tray and tray exit.
- `shutdown-verify.ps1` checks the 15-second countdown exits while hidden.
- `download-while-tray-verify.ps1` runs a local mock NNTP server and verifies a
  download completes while the window is hidden in the tray.
- These three run in a temporary `NQ_CONFIG` folder and never touch `app/*.json`.
- `installer-test.ps1` compiles a test variant of `newsqueue.iss` (name
  `NewsQueue Install Test`, its own AppId), installs it into a temp folder
  over a simulated IExpress install, checks files, registry, shortcuts,
  NQ_WAIT_PID/NQ_RELAUNCH and installed-exe launch, then uninstalls. It never
  touches a real NewsQueue install, its shortcuts or uninstall entry.
- `screenshots.ps1` regenerates `shots/golden/` from the native window (queue,
  PAR2/Log/RSS tabs, every dialog, three Setup pages) and reports how much
  each image changed; `screenshots.ps1 -Check` captures into `shots/check/`
  and fails when an image differs from its golden by more than 0.5 % of
  pixels. `util::capture` takes
  the frame with `PrintWindow(PW_RENDERFULLCONTENT)` and the client area with
  `PW_CLIENTONLY` (DWM's copy can miss controls painted just before); custom
  painted windows answer `WM_PRINTCLIENT`.

## 8. Versioning, packaging and releases

NewsQueue uses independent versions. Current version is **1.0.9** (1.0.2 was the first release that can self-update).
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

The Inno Setup 7 installer (`installer/newsqueue.iss`, since 2026-11-18;
1.0.0-1.0.5 used IExpress) is per-user (`PrivilegesRequired=lowest`):

- installs to `%LOCALAPPDATA%\Programs\NewsQueue`
- Start Menu shortcuts, optional Desktop shortcut (task `desktopicon`)
- writes `HKCU\...\Uninstall\{7A3E2C51-9B4D-4F0E-8C21-6D5B9E1A4F37}_is1`
  and removes the old IExpress `...\Uninstall\NewsQueue` entry,
  `uninstall.ps1` and its Start Menu uninstall shortcut
- `unins000.exe` removes only installed files: settings, queue, history,
  logs and downloads beside the exe stay
- self-update (1.0.6+) runs it with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART
  /MERGETASKS=!desktopicon /DIR=<install>`; 1.0.2-1.0.5 copies start it with
  `NQ_SILENT`/`NQ_INSTALL_DIR`/`NQ_NO_DESKTOP`/`NQ_WAIT_PID`/`NQ_RELAUNCH`
  only, and the `[Code]` section clicks through the wizard for them (a
  running Inno setup cannot start its own exe: access denied)
- `newsqueue.iss` takes optional `/DAppName`, `/DAppGuid` and
  `/DOutputName`; the defaults build the real installer, `installer-test.ps1`
  uses them for its test variant
- the env-var run cannot skip the Ready page; `CurPageChanged` posts a
  `BM_CLICK` to Install there (a direct click inside the event is ignored)

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

The approved desktop reference is the polished **Compact** Win32 mockup in
`app/native-mockups`. Production uses actual common controls v6, system fonts,
OS-managed frames/captions/menus and visual styles. It does not install the old
`classic.rs` process-wide import patches or caption/control subclasses.

- Main toolbar: 30 px drawn icons, 55x46 minimum/autosized text buttons;
  secondary toolbar: 16 px icons, 23x22 buttons. Preserve commands/tooltips.
- Tab pages paint a continuous system button-face background, including filter
  labels and command strips. Edits retain native white surfaces and borders.
- Queue and auxiliary tables use native gridlines, headers, selection and
  scrollbars. Queue progress uses the OS progress theme (green/amber/error),
  with plain text for completed and zero-progress states. Keep row selection,
  expansion, engine actions and saved column/panel dimensions functional.
- Setup retains its 15 real pages and settings bindings. Dialogs use `form.rs`
  and native buttons, checkboxes, combos, inputs and non-client chrome.
- Existing saved queue column widths and connection-panel height are respected;
  the mockup's demonstration data/dimensions do not overwrite user settings.
- `--screenshot` renders sample data off-screen with an unshown owner (no taskbar
  entry or activation). It starts neither the engine thread nor tray. Use
  `NQ_DEMO_TAB`, `NQ_DEMO_DIALOG`, `NQ_DEMO_SETUP_PAGE` to choose the target;
  `NQ_DPI` can force layout scales for captures, defaulting to 96.
- `--ui-smoke` runs production window/control/command checks against the sample
  engine, off-screen, and exits. It does not inject input, connect or persist.
  Suppress the debug console with `Start-Process -WindowStyle Hidden`.
- Keep existing goldens as historical evidence until an intentional baseline
  update. See `app/NATIVE_COMPACT.md` for current verification and limitations.
- The web UI remains egui/WASM. Its current Classic skin stays separate from
  native desktop styling; Caustica + Classic web themes are implemented locally.
  See web/QA_REPORT.md for the hidden browser verification and limits.
- The requested Qt6 version is a separate experiment under `app/qt6-mockup`,
  not a replacement for the Rust engine, production Win32 frontend or egui web.

## 10. Known gotchas

- The engine ticks on its own thread; the main window may be hidden or
  minimized freely (screenshot mode is the exception: no engine thread).
- `ui(|u| ...)` borrows a `RefCell`: never call `SendMessageW`, open a dialog or
  call anything that can re-enter a window proc while inside it.
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
- Release builds have no console window. Debug builds do. A panic writes its
  message and backtrace to `log\crash.log` (`install_crash_log` in `main.rs`)
  before the process ends; a panic in a window procedure cannot unwind.
- `update_menu` (WM_INITMENUPOPUP) once called `current_tab()` inside
  `ui()`, so 1.0.6 crashed whenever a menu opened. A function that calls
  `ui()` must never run inside another `ui()` closure.
- `app/newsqueue-settings.json` and other runtime JSON files are local-only.
- Self-update never runs from dev builds under `target\`; they only show a notice.
  `NQ_UPDATE_REPO=owner/name` overrides the repo for testing.
- The original binaries/evidence are read-only. Use Ghidra/DFM evidence rather
  than guessing measured constants.
- Keep `MECHANICS.md` and `mechanics.json` in sync when a measured value changes.

## 11. Current roadmap state and blockers

`ROADMAP.md` is the active source of priorities and blockers. Historical release
and test evidence is retained there and in `GUI_PLAN.md`; it does not establish
that current desktop or web interactions pass. The local NZB picker fix and its
verification are recorded in `app/PICKER_SMOKE.md`; broader capability and
release gates remain. Do not publish or install over the user's app as part of
diagnostic work.

## 12. Where to start for common changes

- New setting: `model.rs` field + default, a `FIELDS` row in `setup.rs`,
  actual behaviour in the engine/worker, `SETTINGS_CHECKLIST.md`, test.
- New queue action: a `Command` in core if the engine changes, the menu or
  toolbar entry and handler in `main.rs`, test.
- New network feature: `download.rs`, keep the snapshot/channel pattern.
- New post-processing: `unrar.rs`/`par2.rs`, keep `UnrarError` classification,
  add a fixture/test.
- New tray behaviour: `tray.rs` (hide with `SW_HIDE`, show with `SW_SHOW`).
- New shutdown action: `shutdown.rs` plus App countdown/state tests.
- New setup page: `PAGES` and `FIELDS` in `setup.rs`; preserve the original
  order.
- Screenshot/UI change: update `main.rs`/`dialogs.rs`/`setup.rs`, run
  `screenshots.ps1`, inspect `shots/golden/` for regressions.

Verification should match the change: focused tests for affected behavior;
workspace tests and native build for integration, plus a real WASM build for
web/shared API changes; isolated interactions and inspected screenshots for UI
changes. Release checks include packaging, installer, applicable tray/download/
shutdown scripts and Docker smoke on the Linux host. Do not rebuild installers
or overwrite goldens for routine iteration. Documentation-only edits need a
diff/content review, not application tests. Report exact evidence and gaps.
