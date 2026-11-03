# NewsQueue

<img src="app/assets/newsqueue-128.png" width="96" alt="NewsQueue icon">

A from-scratch Rust + egui desktop Usenet downloader modelled on the behaviour
of AltBinz 0.48.4. The UI, queue, settings and measurements live in `app/`.

NewsQueue uses its own versioning: the current release is **0.1.0**, and 1.0.0
will follow once the remaining external validation is complete.

## Features

- Download queue with groups/files, pause/resume, move, rename, delete, export
- Real NNTP downloads over TLS or plain NNTP through direct, HTTP CONNECT or
  SOCKS5 connections, with parallel workers, yEnc decode, resume sidecars,
  missing-article handling, article cache and multi-server support
- Real PAR2 verification/repair through the bundled `par2.exe`, followed by
  optional unrar
- RAR extraction through the bundled 64-bit `UnRAR.exe`, with overwrite/rename,
  strip-root, password, delete-archive, move/delete-pattern, target-folder,
  auto-rename-to-collection, delete-empty-folder and group/NZB subfolder options
- Explorer tab with folder tree, file list, multi-select, back/forward/up history,
  favourites, filters, context menu, rename/delete and configurable columns
- RSS feed fetching with filter-based autodownload
- Generic Newznab search (configure URL/API key in Setup > NZB)
- Per-collection or default execute commands after successful download/PAR2/unrar
- File join/split actions in the queue and Explorer
- Native Windows notification-area icon with show/restore, Exit, minimize-to-tray,
  close-to-tray, speed tooltip and completion balloons (verified by
  `app/tray-verify.ps1`)
- Per-user Windows installer with Start Menu/Desktop shortcuts, Add/Remove
  Programs entry and uninstaller (`app/build-installer.ps1`)
- Setup pages, server editor, scheduler, categories, real persisted statistics,
  search, notes, log, PAR2 list and speed limiting
- `--screenshot <file>` for headless UI verification

## Build and test

Run from `app/`:

```powershell
cargo build
cargo test
cargo run
cargo run -- --screenshot shots/now.png
```

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
cargo run -- --search-test <query>
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

This writes `app/dist/NewsQueue-0.1.0-Setup.exe` and a `.sha256` checksum. The
installer copies the app and tools to `%LOCALAPPDATA%\Programs\NewsQueue`,
creates Start Menu and Desktop shortcuts, registers Add/Remove Programs, and
includes an uninstaller. It does not need administrator rights.

Verify install and uninstall end to end (requires no existing NewsQueue install):

```powershell
.\installer-test.ps1
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
- `ROADMAP.md` - parity plan, remaining gaps and required test data
- `app/MECHANICS.md` - measured behaviour and function addresses from the
  original binary
- `app/mechanics.json` - machine-readable measured values

## Scope

RAR creation is not implemented. PAR2 repair, unrar, RSS autodownload and the
main download path are implemented. The original binaries, reverse-engineering
evidence, build output and local runtime data are excluded from this repository.
