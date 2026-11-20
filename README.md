# NewsQueue

<img src="app/assets/newsqueue-128.png" width="96" alt="NewsQueue icon">

A Rust Usenet downloader with a native Win32 desktop, a headless server and
an egui/WASM web UI. The shared engine, queue and settings live in `app/`.
Originally inspired by AltBinz, NewsQueue now follows its own product direction.

The current release is **1.0.10** for Windows, server and web.

## Download and run

- **Windows x64:** use the [1.0.10 installer or portable ZIP](https://github.com/Kaan88/newsqueue/releases/tag/v1.0.10).
  The native Compact design retains its original Windows font.
- **Docker / web UI:** use the Linux amd64 image archive and Compose files from
  the same release. Follow the [Docker Compose guide](https://github.com/Kaan88/newsqueue/blob/v1.0.10/app/DOCKER.md) for setup,
  authentication, persistent folders, updates and source builds.

Quick start after downloading the image, `compose.yaml` and `env.example`:

```sh
cp env.example .env
# Edit .env and replace NQ_API_KEY with a long random key.
docker load -i NewsQueue-1.0.10-linux-amd64.tar.gz
docker compose up -d --no-build
```

On PowerShell, replace `cp` with `Copy-Item`. Open **http://localhost:8480/**,
sign in with your API key, and configure servers in **Tools → Options**.
The image archive needs no registry account. The default port is loopback-only.

The web UI offers **Caustica** and **Classic**, with independent
**System / Light / Dark** preferences. Windows includes the same web UI
for its optional Web access feature.

For matching source, download `NewsQueue-1.0.10-source.zip` or check out `v1.0.10`.

## New in 1.0.10

- Named multi-file NZBs start transferring without first inspecting every file. Obfuscated jobs begin transferring prepared files while discovery continues. Connection rows show actual preparation, transfer, retry and waiting activity.
- Web Ctrl+C and right-click copying for queue rows, individual files, history, PAR2, logs, RSS and connections.
- Per-job **Unpack after download** overrides the global default and survives restart; queued automatic extraction respects a later Off choice.
- Windows extraction and PAR2 tools run without console windows.

## Controls and download behavior

Desktop **Setup → User interface → Allow only one instance** is enabled by
default, including for existing configurations. Opening NewsQueue again restores
the running window and forwards NZB files to it. The setting applies on the next
launch; separate configuration folders remain independent. The connections
divider no longer grows the toolbar or progressively shrinks the queue.

Desktop and web support custom speed caps in decimal MB/s, applied live
across all download connections. Percentage limits and No limit remain available.
The web connections divider is draggable and remembers its height in the browser.
Caustica uses bundled Inter at 14 px; Classic retains its original font. Native
controls retain the original Windows font and sizing. Both toolbars begin
with Connect and expose the same common actions, including Speed. Desktop update
checking is in About; desktop-only actions remain in menus.

Hold the primary mouse button and drag over settings categories to switch pages.
The web TLS checkbox now switches the standard ports 119/563 while preserving
custom ports. Non-editable web surfaces use an arrow cursor.

**Temporary folder:** this is currently a reserved setting, not an operational
incomplete-download staging area. Partial payloads and `.nqpart` resume sidecars
stay in each job's download folder. The Docker `/incomplete` mount is retained for
compatibility. Custom speed caps are session controls, like percentage caps; the
existing startup-speed setting still determines the limit after a restart.

Download integrity has also been hardened: article sizes/ranges and CRCs are
checked, advertised whole-file CRCs are verified when supplied, and resumed bytes
are rechecked against saved range checksums. `.nqverified` receipts let completed
files be checked before skipping them. Older ID-only `.nqpart` records cannot
prove disk integrity, so their articles are downloaded again. Missing/corrupt
articles remain incomplete and enter the configured PAR2 path.

Queue/settings/history/statistics saves now replace files atomically. Cancelled
workers finish before a replacement download can write to the same output, and
direct-unpack cleanup waits for verified download completion.

### Copying and automatic unpacking

In the web UI, select a queue row or an expanded file and press **Ctrl+C**
to copy its name. Right-click for copying names, details or the download
folder. History, PAR2, log, RSS and connection rows also support copying.
Text fields keep their normal editing shortcuts.

**Unpack downloads by default** sets the global policy. A job's
**Unpack after download** choice overrides that policy and survives restart.
Turning it off also cancels a waiting automatic extraction. **Unpack now**
is an explicit request and is kept even if automatic unpacking is later disabled.
RAR, ZIP/7z and PAR2 tools run without console popups on Windows.

## Features

- Download queue with groups/files, pause/resume, move, rename, delete, export
- Real NNTP downloads over TLS or plain NNTP through direct, HTTP CONNECT or
  SOCKS5 connections, with parallel workers, yEnc decode, resume sidecars,
  missing-article handling, article cache and multi-server support
- Real PAR2 verification/repair through the bundled `par2.exe`, followed by
  optional unrar
- RAR extraction through the bundled 64-bit `UnRAR.exe`, with overwrite/rename,
  strip-root, password prompting, delete-archive, move/delete-pattern,
  target-folder, auto-rename-to-collection, delete-empty-folder and
  archive/group/NZB subfolder options
- RSS feed fetching with filter-based autodownload
- Per-collection or default execute commands after successful download/PAR2/unrar
- Version check against GitHub releases with optional verified self-update
- Native Windows notification-area icon with show/restore, Exit,
  minimize-to-tray and close-to-tray (hidden, no taskbar entry),
  speed tooltip and completion balloons (verified by `app/tray-verify.ps1`)
- Queue-drained Windows shutdown, hibernate, sleep or app exit with a 15-second
  abortable countdown and a mockable backend
- Per-user Windows installer with Start Menu/Desktop shortcuts, Add/Remove
  Programs entry and uninstaller (`app/build-installer.ps1`)
- Setup pages, server editor, scheduler, real persisted statistics,
  notes, log, PAR2 list and speed limiting
- Native Compact Windows UI with responsive menus/scrolling and remembered
  open-file folders; Caustica and Classic web themes with saved preferences
- Linux container with non-root runtime, persistent volumes and API-key login
- `--screenshot <file>` for headless UI verification

## Build and test

For the published build, check out `v1.0.10` first. Run from `app/`:

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

For all workspace packages, including web theme tests, use
`cargo test --workspace -- --test-threads=1`. The 1.0.10 release passed
**162 workspace tests**, hidden desktop and browser checks, installer
upgrade/uninstall verification, and Docker health, authentication and restart
checks. See [the UI verification record](https://github.com/Kaan88/newsqueue/blob/v1.0.10/app/UI_POLISH_AUDIT.md)
for coverage and limits.
Build the web UI on Windows with `.\web\build-web.ps1` (requires the
`wasm32-unknown-unknown` target and wasm-bindgen CLI 0.2.130), or use Docker's
multi-stage build on Linux.

Single tests and diagnostics:

```powershell
cargo test speed_limit_uses_decompiled_formula
cargo test unrar
cargo test par2
cargo test completed_collection_can_be_unpacked
cargo test par2_repair_then_unrar_chain_works
cargo run -- --unrar-test <folder-with-rar> <dest>
cargo run -- --par2-test <folder-with-par2>
cargo run -- --rss-test
cargo run -- --rss-download-test
cargo run -- --download-test <file.nzb>
.\tray-verify.ps1
```

`tray-verify.ps1` starts the built debug app, minimizes and restores it through
the tray, closes it to the tray and exits through the tray command. Close any
running NewsQueue first; the script posts Win32 messages to the test instance.

Package a portable release:

```powershell
.\package.ps1
```

Build the per-user installer:

```powershell
.\build-installer.ps1
```

This writes `app/dist/NewsQueue-1.0.10-Setup.exe` and a `.sha256` checksum. The
installer copies the app and tools to `%LOCALAPPDATA%\Programs\NewsQueue`,
creates Start Menu and Desktop shortcuts, registers Add/Remove Programs, and
includes an uninstaller. It does not need administrator rights.

Verify install and uninstall using an isolated test installation:

```powershell
.\installer-test.ps1 -Hidden
```

The release build uses the Windows GUI subsystem and does not open a console
window. Use the debug build for command-line diagnostics.

Verify the tray and shutdown countdown:

```powershell
.\tray-verify.ps1
.\shutdown-verify.ps1
.\download-while-tray-verify.ps1
```

Regenerate the golden UI screenshots:

```powershell
.\screenshots.ps1
```

## Configuration

Desktop settings, queue, history and statistics are stored in
`newsqueue-settings.json`, `newsqueue-queue.json`, `newsqueue-history.json`
and `newsqueue-stats.json` beside the installed executable. Development builds
use `app/`; `NQ_CONFIG` can select a separate configuration folder. Docker uses
`/config`, backed by the persistent host folder in Compose. These files are
created locally and ignored by git. Download history and transfer statistics are real
and persist across restarts; demo data is only used by `--screenshot`.
Configure your Usenet server and RSS feeds in the app; do not commit
credentials or feed API keys.

`UnRAR.exe` and `par2.exe` are bundled in `app/tools/` with their license files.
The original AltBinz DLLs are 32-bit; this build is 64-bit, so extraction and
repair use the standalone tools.

## Download test

Place an NZB locally and run from `app/`:

```powershell
cargo run -- --download-test ..\Your.Test.Release.nzb
```

This performs real network downloads. Missing articles are skipped and mark the
collection `Incomplete`, which then flows into the PAR2 check/repair stage.

## Documentation

- `AGENTS.md` - commands, layout and gotchas for coding agents
- `ROADMAP.md` - current priorities and verification status
- [Docker Compose guide for 1.0.10](https://github.com/Kaan88/newsqueue/blob/v1.0.10/app/DOCKER.md) - installation and operations
- `app/web/QA_REPORT.md` - theme, layout and functionality verification
- `app/MECHANICS.md` - measured behaviour and function addresses from the
  original binary
- `app/mechanics.json` - machine-readable measured values

## Scope

RAR creation is not implemented. PAR2 repair, unrar, RSS autodownload and the
main download path are implemented. The original binaries, reverse-engineering
evidence, build output and local runtime data are excluded from this repository.
