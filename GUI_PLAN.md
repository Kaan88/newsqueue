# GUI plan: Windows desktop and Docker web

Status: **done (2026-11-18)**. All three phases are implemented; the
sections below keep the plan with a status line each. This replaced the
earlier Win32-only plan.

## Goal

One downloader core with two front ends:

- **Windows desktop**: the AltBinz-style app, rewritten on native Win32.
- **Docker web**: the same engine running headless on Linux (NAS, server),
  controlled from a browser.

This is a personal downloader, not a SABnzbd replacement. No SABnzbd API, no
*arr integration, no categories.

## Why the browser can't simply run the egui app

egui compiles to WebAssembly, but a browser page can't open NNTP (raw TCP/TLS)
sockets, run blocking threads, write gigabytes to a download folder or start
`par2`/`unrar`. The engine must run as a process; the browser is only a remote
control. So the work is: separate the engine, then put a web UI in front of it.

## Layout

```
app/
  core/      newsqueue-core: Engine, download, container, par2, unrar,
             NZB parser, RSS, settings/queue/history/stats. No UI, no Win32.
  src/       newsqueue (Windows, the root package): native Win32 GUI, tray,
             shutdown, installer, self-update.
  server/    newsqueue-server: the JSON API (library + binary). Docker image.
  web/       newsqueue-web: the egui web UI (WebAssembly).
```

(As built, the desktop stayed the root package instead of moving to
`desktop/`, so the installer, scripts and `newsqueue.exe` name are unchanged.)

A Cargo workspace. `core` builds on Windows and Linux; everything
Windows-specific (`tray.rs`, `shutdown.rs`, `update.rs`, the installer,
`windows` crate calls) lives in `desktop`.

## Phase 0: engine split (required for both)

**Status 2026-11-18:** done: the `core` crate; `Engine` with the engine state,
methods and `tick`; `Event`s instead of UI flag writes; `Command`s via `Engine::apply` (step 3); `EngineHost` ticks the engine on its own thread between frames (step 4); `EngineHandle` sends `Command`s and reads a secret-free `Snapshot` from any thread (step 5); tool lookup on PATH
(`par2`/`unrar` on Linux); core builds and passes its tests on Debian 13.
Steps 6–7 done (tool paths are settings; the engine tests moved to `core`).
The native desktop reads the `Snapshot` and sends `Command`s.

Today `App` in `main.rs` holds settings, queue, workers, timers, logs, stats
and dozens of `show_*` dialog flags, and `App::tick` drives everything between
frames.

1. `Engine` in `core` owns settings, queue, history, stats, active
   download/PAR2/unrar workers, log, scheduler, RSS timer, auto-import and
   execute hooks.
2. `Engine::tick(now) -> Vec<Event>` (`QueueChanged`, `LogLine`, `Toast`,
   `PasswordNeeded(id)`, `Finished(id)`, …) replaces UI flag mutation.
3. `Engine::apply(Command)` replaces direct `App` edits, built from the
   current `UiAction` enum (add NZB, start, pause, move, delete, rename,
   set password, change settings, …).
4. `Engine` is driven by its own thread with a timer, not by UI frames. This
   also removes the reason for the off-screen tray hack: the engine keeps
   ticking whatever the window does.
5. Front ends talk to it through a handle: `send(Command)` plus a shared
   read-only `Snapshot` (queue rows, speed, status bar values) and an event
   channel.
6. Tool paths are settings: `UnRAR.exe`/`par2.exe` on Windows, `unrar`/`par2`
   on Linux. Path handling stays `PathBuf`, no `\` literals.
7. Move the `main.rs` integration tests that need no UI to `core`.

Done when: `cargo test` passes on Windows, `cargo build -p newsqueue-core`
passes on Linux, golden screenshots are unchanged, and `core` has no egui or
Win32 imports.

## Phase 1: Docker web front end

**Status 2026-11-18: done.** `app/server`: every endpoint below, API key
(header, query or login cookie), SSE, private settings never sent or
changed, Ctrl+C/SIGTERM saves. `app/web`: the egui web UI (queue tree with
drag reorder, history, PAR2, log, RSS, Options, Add NZB, phone layout),
since 2026-11-18 drawn as a classic Windows 2000 window like the desktop. `Dockerfile` + `compose.yaml`; `docker-smoke.sh` (build, health, egui page,
API key, mock-NNTP download through the API, save on stop) passes on a Debian
Docker host. The API is
also a library, so the desktop app can serve it (Web access).

Server (`newsqueue-server`):

- `tiny_http` (blocking, fits the no-async-runtime rule) on one port,
  default 8480.
- JSON API under `/api/`:
  - `GET  /api/state` — snapshot: queue, speed, status, paused, free space
  - `GET  /api/history`, `GET /api/log?since=n`
  - `POST /api/nzb` (upload) and `POST /api/nzb/url`
  - `POST /api/queue/{id}/{pause|resume|delete|up|down|top|bottom}`
  - `POST /api/queue/{id}/password`
  - `POST /api/pause`, `POST /api/resume`, `POST /api/speed-limit`
  - `GET/PUT /api/settings` (passwords never returned, only "set/not set")
  - `GET /api/events` — Server-Sent Events, so the page updates live
    without polling every second.
- Auth: a login password (hashed in settings) and a session cookie; an API
  key for scripts. Refuse to start without a password unless bound to
  localhost.
- No built-in TLS; put it behind a reverse proxy if exposed. Document that.

Web UI:

- The egui front end compiled to WebAssembly (eframe web), served by the
  server and talking to it only through the JSON API and SSE. The same
  widgets and theme as the desktop app, so look-and-feel work on egui now
  carries over. Fonts are bundled for the web build (no Windows font folder).
- Pages: Queue (collections with expandable files, progress, speed, ETA,
  per-item actions, drag to reorder), History, Log, Add NZB (upload,
  URL, drag-and-drop file), Settings (servers, folders, PAR2/unpack, speed
  limit/scheduler, RSS).
- Usable on a phone. Light/dark from the system.

Docker:

- Multi-stage build: `rust:slim` builds the server; runtime is
  `debian:stable-slim` with `par2` and `unrar` (non-free) or `7zip` as a
  fallback extractor.
- Volumes: `/config` (settings, queue, history, stats, log), `/downloads`,
  `/incomplete`, `/watch` (auto-import folder).
- `PUID`/`PGID`, `TZ`, health check on `/api/health`.
- Ship a `docker-compose.yml` example.
- Updates come from new image tags; the self-updater is not built in.

Testing:

- API tests against the mock NNTP server already in `download.rs`
  (add → download → history).
- A container smoke test: `docker build`, run, upload a mock NZB, wait for
  Completed.

## Phase 2: Windows desktop front end (native Win32)

**Status 2026-11-18: done.** The desktop app (`app/src`) is native Win32
through the `windows` crate; eframe/egui/egui_extras/rfd are gone from it.
`main.rs` (window, toolbars, tabs, queue tree, status bar, tray glue),
`queue.rs` (pure, tested tree/cell/redraw logic), `form.rs` (dialog
framework and control helpers), `setup.rs` (the 15 Setup pages from the
`FIELDS` table, server editor, scheduler grid), `dialogs.rs` (stats, columns,
pause-for, speed limit, shutdown countdown, version/update, about, notes,
rename, execute, unrar password, RSS feeds, filter help), `pick.rs`
(`IFileOpenDialog`/`IFileSaveDialog`). Verified: `download-while-tray-verify.ps1`
downloads from the mock NNTP server while hidden in the tray,
`tray-verify.ps1` and `shutdown-verify.ps1` pass, `screenshots.ps1`
regenerates `shots/golden/` with `PrintWindow`, and `installer-test.ps1`
passes against the native build (a test-named installer variant, so it runs
beside a real install). Web access: Setup → Miscellaneous #3 starts the
`newsqueue-server` API and web UI inside the desktop app. The original
proposal follows.

The desktop app is rewritten on native Win32. This is required, not an
experiment: egui is removed from the desktop build at the end of this phase.
It keeps the current features: AltBinz layout, tray, shutdown countdown,
installer and self-update.

Why: AltBinz is Delphi VCL over the Win32 common controls, so native
ListView/TreeView, toolbar, tabs and status bar recreate it most faithfully.
It also brings real ClearType text, per-monitor DPI, keyboard navigation and
accessibility, plus a virtual ListView that handles any queue size.

Crate: `windows` (windows-rs) with small in-house wrappers; `winsafe` if raw
windows-rs is too verbose. Not `native-windows-gui` (unmaintained).

- Step 1: the egui app moves to `desktop` and drives the `Engine` handle,
  unchanged, so there is a working app throughout the rewrite.
- **No dark mode (decided 2026-11-18).** Win32 common controls have no
  complete dark theme (a test left tabs, status bar, edits and progress bars
  light or broken); the native app is light-only with the classic look
  (visual styles off for the client area, a menu bar).
  Mockups from real controls: `mockups/win32/shots/` (`nqmock --capture`).
- Step 2, main window and queue: toolbar with the measured buttons and
  drawn icons, tab control, queue secondary toolbar, status bar with the
  measured panes. Queue is a virtual ListView (`LVS_REPORT | LVS_OWNERDATA`)
  using `QUEUE_COLUMN_WIDTHS`, collections shown with indented file rows and
  an expand glyph, progress custom-drawn (`NM_CUSTOMDRAW`). A `WM_TIMER`
  redraws only changed rows from the `Snapshot`. Per-monitor v2 DPI manifest.
- Step 3, tabs: PAR2, Log (virtual ListView), RSS (feed + item ListViews).
- Step 4, dialogs: a layout helper places controls at the coordinates from
  the DFM evidence (`investigation.cany/`, `reference/`). Setup is a modeless
  dialog with the 15 pages in the original order, bound to `Settings` by a
  `bind_check`/`bind_edit`/`bind_combo` table. Then the server editor, stats,
  columns, rename, unrar password, shutdown countdown, execute command, RSS
  feeds, speed limit, version/update and the rest. File and folder pickers
  through `IFileOpenDialog` (replaces `rfd`).
- Step 5, tray and cleanup: keep the `Shell_NotifyIconW` code but hide with
  `SW_HIDE`; drop the off-screen placement and `ITaskbarList3` tab removal.
  Rewrite `tray-verify.ps1`, `download-while-tray-verify.ps1` and
  `screenshots.ps1` (`PrintWindow` captures, compared with a tolerance),
  regenerate `shots/golden/`, then remove eframe/egui/egui_extras and update
  CLAUDE.md, AGENTS.md and ROADMAP.md.
- Optional: a "Web access" setting that starts the same server inside the
  desktop app, so the Windows install can also be controlled from a phone.

Done when: the native build downloads a mock-NNTP collection end to end,
every dialog exists, the tray/shutdown/installer scripts pass, the golden
screenshots are regenerated from the native window, and no egui dependency
remains in `desktop`.

## Order and effort

1. Phase 0 engine split: ~1 week.
2. Phase 1 server + web UI + Docker: ~1–2 weeks.
3. Phase 2 step 1, egui desktop on the engine: a few days.
4. Phase 2 steps 2–5, native Win32 rewrite: 3–4 weeks.

## Risks

- **Engine split touches most of `main.rs`.** Do it in small steps with
  `cargo test` and golden screenshots after each.
- **Linux `unrar` licensing:** non-free; the image may need `7zip` as the
  default and `unrar` optional.
- **Exposed web UI** holds Usenet credentials. Password required, secrets
  never returned by the API, reverse proxy for TLS.
- **Win32 setup pages are a lot of mechanical work;** the binding table
  keeps it manageable.
- **Owner-data ListView with a tree:** AltBinz fakes it with indentation;
  expand/collapse and selection need care.
- **Real-window screenshot tests are flakier** (font smoothing, DPI); use a
  tolerance.
- **Two front ends drift.** Both only send `Command`s and read `Snapshot`s;
  no logic in either front end.
