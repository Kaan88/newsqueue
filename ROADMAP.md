# NewsQueue parity roadmap

Goal: **behavioural parity** with AltBinz 0.48.4 — same workflow, settings and
results, implemented from the measured evidence in `app/MECHANICS.md`. This is
not a source-code or artwork copy.

Current state: usable clone. Strong on layout, queue/status maths, basic NNTP,
unrar extraction and common Explorer browsing. Weak on PAR2, resilient
downloads, RSS/search, categories and shell-grade Explorer behaviour.

Effort: **S** ≈ one focused session, **M** ≈ 2–3, **L** ≈ 4+ and needs repeated
live testing.

## Phase 0 — verification baseline (S, required first)

- Keep a golden screenshot set for every tab and major dialog.
- Add a feature checklist to CI/manual verification with pass/fail per setting.
- Add fixtures: small complete NZB, multi-volume RAR set, healthy PAR2 set,
  deliberately damaged PAR2 set.
- Add a tiny mock NNTP server for 430/timeout/retry/resume tests so CI does not
  depend on a live Usenet account.
- **Done when:** `cargo test`, screenshot, and the manual checklist run without
  live credentials.

## Phase 1 — resilient downloader (L, highest priority)

- Resume partial files instead of truncating them on every start.
- Retry/backoff for timeouts and transient failures; honour retry settings.
- Handle `430 Article not found` by recording missing segments, continuing, and
  marking the collection `Incomplete` instead of aborting the whole job.
- Multiple enabled servers, levels, priorities and failover; use all configured
  connections, not only the first server.
- Separate decode/article-cache workers and wire thread-priority settings.
- Auto-import watched NZB folder; execute command after download/par2/unrar;
  cleanup/delete rules.
- **Done when:** a 2 GB multi-volume post survives a killed process, a forced
  retry, and one missing article, and still reaches PAR2/unrar.

## Phase 2 — real PAR2 pipeline (L, highest priority)

- Bundle or build a `par2` binary and run real check/repair operations.
- Parse output into blocks, progress, status (`Checking`, `Repairing`, `OK`,
  `Incomplete`, `Repair failed`).
- Import PAR2/SFV sets, associate them with collections, run after download when
  `PAR2checking` / `PAR2Repairing` are enabled.
- Cleanup after repair, delete-small-PAR2, save PAR2 info, pause unrar until
  repair completes.
- Make the PAR2 tab actions real (`Repair`, `Unpack`, delete, import).
- **Done when:** a deliberately damaged file is repaired from a PAR2 set, a
  healthy set reports `All files OK`, and unrar starts only after repair.

## Phase 3 — unrar/post-processing parity (M)

- Add remaining original options: auto-rename to collection, delete empty
  folder, group/NZB subfolders, both delete-archive flags, delete-after-unpack
  and move-to-unrar patterns.
- Prompt for passwords; store per-collection passwords.
- Trigger after PAR2, from the PAR2 tab, and from the queue context menu.
- Handle cancellation, existing files, multi-volume gaps and failure cleanup.
- **Done when:** each option has an automated or manual test and `Unpack now`
  works on a real downloaded set.

## Phase 4 — RSS and search (L)

- Real RSS/Atom fetching and refresh timer.
- Implement the original filter grammar (`| A/D/F/P/R`, download/unrar folders,
  execute command) and autodownload decisions.
- Real global search: Newznab/search-provider configuration, history, dropdown
  and add-to-queue.
- **Done when:** a feed fixture produces the expected accept/deny decisions and
  a test search endpoint can add a real result to the queue.

## Phase 5 — Explorer shell parity (M)

- Rename, multi-select, cut/copy/paste, drag/drop, open-with and shell properties.
- Full drive/favourites tree with expandable folders and shell namespace.
- File-type icons, sorting, refresh, filter timer and persisted columns.
- **Done when:** the common Explorer actions work without falling back to
  Windows Explorer.

## Phase 6 — settings, history and auxiliary features (M)

- Enforce settings that are currently UI-only: proxy, thread priorities,
  auto-import, tray/notifications, execute/player, log retention, history
  retention.
- Persist real download history and statistics; remove seeded/approximate data.
- Apply category keyword/folder rules on import and manual category changes.
- Real tray, Windows shutdown, Join/Split and the remaining shortcuts.
- **Done when:** every visible setting has an observable effect and a test.

## Phase 7 — visual and release polish (M)

- Screenshot diffs for every tab/dialog; fix layout drift.
- Error dialogs and recovery paths matching the original where practical.
- Portable release zip, installer, code signing (if desired), release build.
- **Done when:** the parity checklist is green on a clean Windows machine.

## What I need from you

### Critical: test data

1. **A complete, currently available NZB/post** small enough to test end to end
   (roughly 100 MB–2 GB) with a multi-volume RAR set and PAR2 files.
   The current `Resident.Evil...` post returns `430 Article not found` on part
   02, so it cannot validate download → repair → unrar.
2. **A damaged variant** of that set (delete/corrupt one file) plus its PAR2
   files, for repair testing.
3. A known-good single-file NZB for basic decode/progress tests.
4. Either a second server account or permission to use a mock server for
   failover/multi-server tests.

### Licensing / packaging decisions

5. Confirm bundling the WinRAR `UnRAR.exe` is acceptable (currently done, with
   licenses included).
6. Choose the PAR2 backend: bundle `altbinz_original/par2/par2.exe` (GPL) or
   build an open-source `par2cmdline` binary. This affects redistribution.
7. Confirm whether native tray notifications, Windows shutdown and execute
   hooks are required for your daily use.

### External services

8. RSS feed URLs and expected filter examples.
9. Newznab/search endpoint and API key, if real global search is required.
10. Whether search should stay generic or match the original provider list.

### Acceptance

11. Which phases are must-have for you: core download/PAR2/unrar only, or the
    full RSS/search/Explorer/settings surface.
12. Target remains Windows-only, correct? Explorer shell integration and tray
    behaviour are Windows-specific.

## Suggested order

**Phase 0 → 1 → 2 → 3**, then decide whether RSS/search/Explorer polish is
needed. Download robustness and PAR2 are the two gaps that most affect whether
this can replace the original for real downloads.
