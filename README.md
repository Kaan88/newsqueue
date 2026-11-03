# NewsQueue

<img src="app/assets/newsqueue-128.png" width="96" alt="NewsQueue icon">

A from-scratch Rust + egui desktop Usenet downloader modelled on the behaviour
of AltBinz 0.48.4. The UI, queue, settings and measurements live in `app/`.

## Features

- Download queue with groups/files, pause/resume, move, rename, delete, export
- Real NNTP downloads over TLS or plain NNTP, parallel connections, yEnc decode
- RAR extraction through the bundled 64-bit `UnRAR.exe`, with overwrite/rename,
  strip-root, password, delete-archive, move/delete-pattern and target-folder
  options
- Explorer tab with folder tree, file list, back/forward/up history, favourites,
  filters, context menu and configurable columns
- Setup pages, server editor, scheduler, categories, statistics, search, notes,
  log, PAR2 list and speed limiting
- `--screenshot <file>` for headless UI verification

## Build and test

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
cargo test completed_collection_can_be_unpacked
```

## Configuration

The app reads `app/newsqueue-settings.json` and `app/newsqueue-queue.json`.
Both are created locally and are ignored by git. Configure your Usenet server
in Setup; do not commit credentials.

`UnRAR.exe` is bundled in `app/tools/` with its license files. The original
AltBinz DLL is 32-bit; this build is 64-bit, so extraction uses the standalone
64-bit tool.

## Download test

The repo contains a large NZB test fixture at the project root. To exercise the
downloader from `app/`:

```powershell
cargo run -- --download-test ..\Resident.Evil.2026.1080p.MA.WEB-DL.H264.SDR.DDP.Atmos.5.1.English.--HONE.nzb
```

This performs real network downloads. If the server returns `430 Article not
found`, the post is incomplete on that server; the downloader reports the error
instead of writing a corrupt file.

## Documentation

- `AGENTS.md` — commands, layout and gotchas for coding agents
- `app/MECHANICS.md` — measured behaviour and function addresses from the
  original binary
- `app/mechanics.json` — machine-readable measured values

## Scope

PAR2 repair and RAR creation are not implemented. The original binaries,
reverse-engineering evidence, build output and local runtime data are excluded
from this repository.
