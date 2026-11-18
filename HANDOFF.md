# Handoff — 2026-11-18

State of NewsQueue at the end of this session, for whoever picks it up next.
Read `CLAUDE.md` first; `ROADMAP.md` and `GUI_PLAN.md` hold the plans.

## Released

**v1.0.5** is on GitHub (zip, setup, `.sha256`):
https://github.com/Kaan88/newsqueue/releases/tag/v1.0.5

- Instrument skin (light/dark/system, egui 0.37), merged from the
  `skin-instrument` branch.
- Engine on its own thread (see below).
- Installer fix: IExpress now launches `.\install.cmd`. A bare `install.cmd`
  silently did nothing (exit 0) when `NoDefaultCurrentDirectoryInExePath` is
  set; Claude Code sessions set it, and some hardened Windows setups do too.
  Older setups (≤1.0.4) have the bug.
- Verified before publishing: 76 tests, `tray-verify`, `download-while-tray-verify`,
  `shutdown-verify`, `installer-test` all passed.

## Engine split (GUI_PLAN Phase 0) — steps 1–5 done

- `core/src/command.rs`: `Command` enum + `Engine::apply`, collections
  addressed by stable `id`. Desktop queue actions, rename, execute, unrar
  password all go through it. `AddNzbFile(path, paused)` added for the server.
- `core/src/host.rs`: `EngineHost`. A thread named `engine` ticks every 50 ms
  while the engine is checked in. The desktop checks it out at the start of
  each frame and back in at the end (`App::logic`/`ui` on eframe 0.37), and
  `App::drop` checks it out before saving. **Any new code path that touches
  `app.engine` outside a frame must call `checkout()` first** or it panics.
- `core/src/snapshot.rs`: secret-free `Snapshot` (queue rows, speed, limit,
  paused/connected, last 50 log lines) and `EngineHandle` (send `Command`,
  read snapshot) usable from any thread.
- Still open: the egui UI reads the engine directly, not the snapshot
  (optional for the desktop); steps 6–7 (tool paths as settings, moving
  UI-free tests from `main.rs` to core).
- The off-screen tray hack is still in place; with the engine thread it is no
  longer strictly needed, but removing it was not attempted.

## Web server (GUI_PLAN Phase 1) — first slice

`app/server` = `newsqueue-server` (workspace member, `tiny_http`, no async).

```
newsqueue-server [--bind 127.0.0.1:8480] [--config DIR] [--api-key KEY]
```

- `GET /api/health` (open), `GET /api/state` (snapshot JSON),
  `POST /api/{pause,resume,connect,disconnect,cleanup}`,
  `POST /api/speed-limit?tag=N|none`,
  `POST /api/nzb?name=x.nzb&paused=1` (raw NZB body, parsed and rejected with
  400 before queueing; saved to `<config>/nzb/uploads/`),
  `POST /api/queue/{id}/{pause|resume|delete|up|down|top|bottom|unpack|password}`.
- Auth: API key in `X-Api-Key` or `?apikey=`; refuses a non-loopback bind
  without one. No TLS (reverse proxy).
- `NQ_CONFIG` / `--config` overrides `app_base()`. **Always pass `--config`
  to a dev server**, otherwise it shares the desktop's `app/` files.
- Engine `Event`s are printed to stdout (password needed, toasts…).
- Tested: 4 unit tests + a live HTTP smoke test with a throwaway config.
- Next: SSE `/api/events`, history/log/settings endpoints (never return
  passwords), `/api/nzb/url`, login + cookie, egui-WASM web UI, Dockerfile +
  compose, container smoke test. Graceful shutdown (Ctrl+C → `save_all`) is
  missing; it relies on the engine's periodic autosave.

## Native Win32 (GUI_PLAN Phase 2) — resumed

First slice in `app/win32` (see GUI_PLAN Phase 2): main window, queue/log,
actions, add NZB, password prompt, driven by the real engine. Verified: builds,
`--capture` with a queued NZB, queue saved on exit. Toolbar/menu clicks were
not driven by a test. Next: tray (`SW_HIDE`), setup dialog, file rows.

### Mockups

- `mockups/win32/` is a standalone throwaway crate (`windows` 0.61, own
  `[workspace]`, not part of `app/`). `cargo build; target\debug\nqmock.exe
  --capture` renders real comctl32 v6 controls and writes
  `mockups/win32/shots/01..07-*.png` (main queue, log, setup download/servers,
  server editor, unrar password, shutdown).
- Decision: **no dark mode** for the native app (common controls have no
  complete dark theme). The egui app keeps light/dark/system.
- Known mockup flaw: the progress label straddling the fill is hard to read.

## Planned: Inno Setup 7 installer

Inno Setup 7 is installed at `C:\Program Files\Inno Setup 7` (`ISCC.exe`).
Plan is in `ROADMAP.md` → "Installer: Inno Setup 7". Key risks: Inno's
uninstall key is `<AppId>_is1` (the first Inno install must remove the old
IExpress `NewsQueue` entry and shortcuts), and self-update from 1.0.2+ must
keep working (`/VERYSILENT`, wait-for-PID, relaunch).

## Gotchas learned this session

- PowerShell 5.1: don't add `2>&1` to cargo/native commands; stderr lines
  become errors and stop `$ErrorActionPreference='Stop'` scripts.
- `installer-test.ps1` refuses to run while a real NewsQueue install is
  registered; the user uninstalled theirs for the 1.0.5 run (reinstall needed).
- Through `Deref` to `EngineHost`, two `app.engine.x` borrows in one
  expression no longer split; read the value into a local first.

## Verification commands

```powershell
cd app
cargo test                      # 80 tests: 43 core, 33 desktop, 4 server
cargo run -- --screenshot shots/now.png
.\tray-verify.ps1; .\download-while-tray-verify.ps1; .\shutdown-verify.ps1
.\build-installer.ps1; .\installer-test.ps1
```
