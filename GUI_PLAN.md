# GUI plan: Windows desktop and Docker web

Status: proposal (2026-11-18). Nothing here is implemented yet. This replaces
the earlier Win32-only plan.

## Goal

One downloader core with two front ends:

- **Windows desktop**: the AltBinz-style app, as today.
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
  desktop/   newsqueue (Windows): GUI, tray, shutdown, installer, self-update.
  server/    newsqueue-server: HTTP server + embedded web UI. Docker image.
```

A Cargo workspace. `core` builds on Windows and Linux; everything
Windows-specific (`tray.rs`, `shutdown.rs`, `update.rs`, the installer,
`windows` crate calls) lives in `desktop`.

## Phase 0: engine split (required for both)

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

- Plain HTML/CSS/JS, embedded in the binary with `include_str!`. No build
  step, no npm.
- Pages: Queue (collections with expandable files, progress, speed, ETA,
  per-item actions, drag to reorder), History, Log, Add NZB (upload,
  URL, drag-and-drop file), Settings (servers, folders, PAR2/unpack, speed
  limit/scheduler, RSS).
- Usable on a phone. Light/dark from the system.
- It does not copy the AltBinz look; that stays the desktop app's job.

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

## Phase 2: Windows desktop front end

Keeps the current features: AltBinz layout, tray, shutdown countdown,
installer and self-update.

- Step 1: the egui app moves to `desktop` and drives the `Engine` handle.
  Behaviour and golden screenshots must not change.
- Step 2: the tray hides the window with `SW_HIDE` and drops the off-screen
  placement and `ITaskbarList3` tab removal, since the engine no longer
  depends on frames. Rewrite `tray-verify.ps1` and
  `download-while-tray-verify.ps1` for this.
- Step 3 (optional): a native Win32 spike, as the earlier plan described —
  virtual ListView queue, Win32 toolbar/tabs/status bar, dialogs at the
  measured DFM coordinates, `PrintWindow` screenshot tests. Go only if it
  clearly beats egui next to `shots/golden/queue.png`; otherwise polish egui
  (Segoe UI, DPI-correct scaling, crisp icons, hover/pressed states).
- Optional: a "Web access" setting that starts the same server inside the
  desktop app, so the Windows install can also be controlled from a phone.

## Order and effort

1. Phase 0 engine split: ~1 week.
2. Phase 1 server + web UI + Docker: ~1–2 weeks.
3. Phase 2 step 1–2 desktop on the engine: a few days.
4. Phase 2 step 3 native Win32: 3–4 weeks, optional.

## Risks

- **Engine split touches most of `main.rs`.** Do it in small steps with
  `cargo test` and golden screenshots after each.
- **Linux `unrar` licensing:** non-free; the image may need `7zip` as the
  default and `unrar` optional.
- **Exposed web UI** holds Usenet credentials. Password required, secrets
  never returned by the API, reverse proxy for TLS.
- **Two front ends drift.** Both only send `Command`s and read `Snapshot`s;
  no logic in either front end.
