# AGENTS.md

## Project

- `app/` is the whole Rust 2021 project (eframe/egui 0.29): a from-scratch clone of AltBinz, presented as NewsQueue 0.48.4.
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
- `cargo run -- --search-test <query>` queries the configured Newznab endpoint.
- `cargo fmt --check` currently fails on existing code. Run `cargo fmt` when formatting; don't claim the check passed unless you ran it.

## Where things live

- `main.rs` — `App` state, CLI entry, key handling, and `App::tick()`: all timers, scheduler, free-space pause, worker start/finish, logging and autosaves flow through here.
- `model.rs` — persisted types (`Settings`, `Collection`, `Server`), measured constants and formatters, NZB parser, sample data. New persisted fields need defaults (`#[serde(default)]`) so existing JSON still loads.
- `ui.rs` — main window, tabs, queue table, status bar. Queue and Explorer rows add a transparent `row_click_overlay` so clicks on labels and progress bars reach the row; preserve it when editing columns.
- `dialogs.rs` — 17 setup pages, server editor, stats/search/columns and other modal windows.
- `download.rs` — real NNTP/yEnc downloader. One active job at a time; all enabled servers are used and the global limit is split across workers. Resume metadata lives in `.nqpart` sidecars. Network code runs in std threads with a `Mutex<DownloadSnapshot>`; there is no async runtime.
- `par2.rs` — PAR2 verify/repair worker using `app/tools/par2.exe`; feeds the unrar stage.
- `rss.rs` — RSS/Atom fetch, filter matching and NZB autodownload using `ureq` with native TLS.
- `search.rs` — generic Newznab XML search and NZB fetch.
- `unrar.rs` — extraction worker for completed collections. Uses the vendored 64-bit `app/tools/UnRAR.exe` (the original's bundled DLL is 32-bit) and a polling channel; do not delete the tool.
- `theme.rs` — measured colors, fonts, icons and widget metrics.

## Gotchas

- Normal runs persist to `app/newsqueue-settings.json`, `app/newsqueue-queue.json` and `app/newsqueue-history.json` (autosave every 30 s / `autosave_minutes`, and on exit). Don't hand-edit them while the app is running. Saved queue entries without segment IDs are discarded on load; sample data appears only in screenshot mode.
- `app/download`, `app/log`, `app/nzb` and `app/temp` are runtime output, not source. `*.nqpart` files are resume sidecars; keep them until a download finishes.
- Test Usenet account, RSS API keys and Newznab API keys are configured only in the local, gitignored `app/newsqueue-settings.json`. Never copy them into source, docs, logs, commits or replies.
- `cargo run -- --download-test <file.nzb>` and Connect perform real network downloads into the download folder; don't run them unless asked. Use a local NZB such as `Civilization.IV-RELOADED.nzb` or `Love.Under.the.Mistletoe...nzb` (both ignored by git). Missing articles are skipped and mark the collection `Incomplete`; PAR2 repair and RAR extraction are real.
- For screenshots of specific UI, set `NQ_DEMO_DIALOG` (`setup`, `server`, `stats`, `search`, `categories`, `notes`, `about`, `columns`, `pause`), `NQ_DEMO_TAB` (`par2`, `explorer`, `log`, `rss`), `NQ_DEMO_SETUP_PAGE=0..16`, `NQ_DEMO_EXPLORER=<folder>`, or `NQ_DEMO_CONNECT=1`.
