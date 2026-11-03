# NewsQueue parity roadmap

Goal: **behavioural parity** with AltBinz 0.48.4 — same workflow, settings and
results, implemented from the measured evidence in `app/MECHANICS.md`. This is
not a source-code or artwork copy.

## Current status (2026-11-03)

- **Phase 0 — partial.** 19 tests, screenshot checks, real PAR2/damaged-RAR
  fixture and the user-provided NZB fixtures exist. A mock NNTP server is still
  missing.
- **Phase 1 — mostly done.** Missing articles no longer abort a job; resume
  sidecars (`.nqpart`) preserve downloaded articles across restarts; all enabled
  servers are used; `Incomplete` feeds the PAR2 stage. Retry/backoff, article
  cache/priorities and execute hooks remain.
- **Phase 2 — done for the core chain.** `app/tools/par2.exe` verifies and
  repairs real sets. A damaged RAR is repaired and then extracted in tests.
  Download → PAR2 → unrar is wired through `App::poll_par2`.
- **Phase 3 — core done.** Overwrite/rename, strip-root, password, delete
  archives, move/delete patterns, target folder, pause-while-unraring and manual
  unpack work. Auto-rename/delete-empty-folder/subfolder variants remain.
- **Phase 4 — RSS fetch and autodownload work.** Feed URLs and filters are
  stored locally (never committed); a live feed returned 100 items and the
  filter test downloaded one NZB. Newznab/global search remains sample-only.
- **Phase 5 — improved.** Explorer has selection, navigation, history,
  favourites, context menu, delete, rename and columns. Drag/drop, open-with,
  multi-select and clipboard cut/copy/paste remain.
- **Phase 6 — partial.** Log retention, persistent history, category routing and
  multi-server use are implemented. Proxy, thread priorities, tray, Join/Split
  and execute/player hooks remain.
- **Phase 7 — partial.** Original icon and window/exe resource are done. Installer
  and screenshot diffs remain.

The supplied `Civilization.IV` and `Love.Under.the.Mistletoe` downloads complete
at the transfer layer, but their PAR2 packet hashes / RAR payloads are
inconsistent; the PAR2 repair path is verified with the generated damaged-RAR
fixture instead.

Current state: usable clone. Strong on layout, queue/status maths, NNTP, PAR2,
unrar extraction and common Explorer browsing. Weak on real search, tray,
drag/drop and a few peripheral settings.

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

## What is still needed

### Test data

1. The two new NZBs download successfully, but their PAR2 packet hashes and RAR
   payloads are inconsistent on the server. A **different complete release with a
   valid PAR2 set** is needed to validate live repair; the generated damaged-RAR
   fixture already covers the repair logic.
2. No second server is available. Multi-server scheduling is implemented, but a
   live failover test needs either a second account or a small mock NNTP server.

### External services

3. The supplied RSS feed works (100 items, one NZB downloaded through a filter).
   Its API key is stored only in the local, gitignored settings file.
4. A Newznab/search endpoint and API key are still needed for real global search;
   the current Search window remains seeded.

### Scope decisions

5. Private use and Windows-only are accepted. `UnRAR.exe` and the bundled PAR2
   binary are included with their license files.
6. Remaining optional parity work: native tray/notifications, drag/drop and
   open-with in Explorer, Join/Split, execute/player hooks, proxy and thread
   priorities. Confirm which of these matter for daily use.

## Suggested next order

**Validate a different real NZB → add mock NNTP retry/failover tests → finish
Explorer drag/drop and open-with → Newznab search → tray/execute polish.**
Download robustness and PAR2 are already in place; the remaining gaps are mostly
peripheral or need external service details.
