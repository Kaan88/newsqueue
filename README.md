# NewsQueue

<img src="app/assets/newsqueue-128.png" width="96" alt="NewsQueue icon">

A Rust Usenet downloader with a native Win32 desktop, a headless server and
an egui/WASM web UI. The shared engine, queue and settings live in `app/`.
Originally inspired by AltBinz, NewsQueue now follows its own product direction.

The current release combines **Windows 1.0.8** with **server/web 1.0.8-web.1**.

## Download and run

- **Windows x64:** use the [1.0.8 installer or portable ZIP](https://github.com/Kaan88/newsqueue/releases/tag/v1.0.8).
  These are the existing, tested native Compact release binaries, unchanged.
- **Docker / web UI:** use the Linux amd64 image archive and Compose files from
  the same release. Follow the [Docker Compose guide](app/DOCKER.md) for setup,
  authentication, persistent folders, updates and source builds.

Quick start after downloading the image, `compose.yaml` and `.env.example`:

```sh
cp .env.example .env
# Edit .env and replace NQ_API_KEY with a long random key.
docker load -i NewsQueue-1.0.8-web.1-linux-amd64.tar.gz
docker compose up -d --no-build
```

On PowerShell, replace `cp` with `Copy-Item`. Open **http://localhost:8480/**,
sign in with your API key, and configure servers in **Tools → Options**.
The image archive needs no registry account. The default port is loopback-only.

The updated web UI restores **Caustica** alongside **Classic**, with independent
**System / Light / Dark** preferences. It also fixes asynchronous server saves
and preserves stored credentials when editing or removing server rows.
Classic's layout and appearance remain intact. The existing Windows assets
retain their originally bundled web UI; use the Docker build for the complete
updated server/web pair.

For the matching source, download `NewsQueue-1.0.8-web.1-source.zip` or check
out `web-v1.0.8-1`. The original `v1.0.8` tag remains tied to the Windows build.

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

Run from `app/`:

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

For all workspace packages, including web theme tests, use
`cargo test --workspace -- --test-threads=1`. The latest theme/API pass has
133 passing tests plus hidden browser visual and interaction checks; see
[the detailed web QA report](app/web/QA_REPORT.md) for coverage and limits.
Build the web UI on Windows with `app/web/build-web.ps1` (requires the
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

This writes `app/dist/NewsQueue-1.0.8-Setup.exe` and a `.sha256` checksum. The
installer copies the app and tools to `%LOCALAPPDATA%\Programs\NewsQueue`,
creates Start Menu and Desktop shortcuts, registers Add/Remove Programs, and
includes an uninstaller. It does not need administrator rights.

Verify install and uninstall end to end (requires no existing NewsQueue install):

```powershell
.\installer-test.ps1
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

The app reads `app/newsqueue-settings.json`, `app/newsqueue-queue.json`,
`app/newsqueue-history.json` and `app/newsqueue-stats.json`. They are created
locally and ignored by git. Download history and transfer statistics are real
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
- `app/DOCKER.md` - Docker Compose installation and operations
- `app/web/QA_REPORT.md` - theme, layout and functionality verification
- `app/MECHANICS.md` - measured behaviour and function addresses from the
  original binary
- `app/mechanics.json` - machine-readable measured values

## Scope

RAR creation is not implemented. PAR2 repair, unrar, RSS autodownload and the
main download path are implemented. The original binaries, reverse-engineering
evidence, build output and local runtime data are excluded from this repository.
