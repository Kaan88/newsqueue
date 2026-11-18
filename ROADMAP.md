# NewsQueue parity roadmap

Goal: **behavioural parity** with AltBinz 0.48.4 — same workflow, settings and
results, implemented from the measured evidence in `app/MECHANICS.md`. This is
not a source-code or artwork copy.

## Current status (2026-11-18)

Self-update confirmed working by the user (1.0.2 → 1.0.3). 1.0.4 is the
first release built on the core `Engine`. 1.0.5 adds the Instrument skin
(light/dark/system, egui 0.37) and runs the engine on its own thread.

**Phase 1 started 2026-11-18:** `app/server` (`newsqueue-server`) serves `/api/health`, `/api/state`, pause/resume/connect/cleanup/speed-limit, `/api/nzb` upload and per-collection queue actions behind an API key; next: SSE events, history/log/settings endpoints, login, the web UI and Docker. GUI_PLAN Phase 0 steps 3–5 done 2026-11-18 (`Command` + `Engine::apply`, `EngineHost` timer thread, `EngineHandle` with `Snapshot`); the egui UI still reads the engine directly (the redesign has landed; switching is optional for the desktop).

Engine split
started: `app/core` (`newsqueue-core`) holds the platform-neutral modules;
`Engine` (with `Event`s) is in core and the desktop app runs on it;
`Command`/`Snapshot`/engine thread from GUI_PLAN Phase 0 are not done yet. Core
builds and passes all 40 tests on Debian 13 (x86_64) with distro `par2` and
`unrar`; the unrar destination separator was a Linux bug and is fixed.

Current app version: **1.0.5** (2026-11-18, Instrument skin + engine thread; 1.0.2 was the first self-updating release), versioned independently of the AltBinz 0.48.4
parity target. `v1.0.0` was deleted and rebuilt on 2026-11-04 because its
binaries corrupted every download; `v1.0.1` supersedes it and adds
content-based naming for obfuscated posts.

### Status at a glance

| Area | State | Notes |
|---|---|---|
| Download engine | Done | TLS/plain NNTP, HTTP CONNECT/SOCKS5, multi-server workers, yEnc, resume sidecars, missing-article handling, article cache, retry/backoff, priorities, content-based naming for obfuscated posts |
| PAR2 | Done for the core chain | Real verify/repair, damaged-RAR fixture, download → PAR2 → unrar |
| Unrar | Done for the common path | Overwrite/rename, strip-root, archive/group/NZB subfolders, auto-rename, delete-empty-folder, password prompt and retry, delete/move patterns, target folder |
| RSS | Done | Live feed returned 100 items; filters downloaded a real NZB |
| Search | Removed | Global/Newznab search was cut on 2026-11-18 |
| Explorer | Removed | See `REMOVED.md` |
| Tray | Done | Off-screen minimize-to-tray with taskbar entry removed, speed tooltip, download/unrar balloons, Show/Exit |
| Shutdown | Done | Shutdown/hibernate/sleep/exit after a 15-second abortable countdown; mockable backend |
| Statistics/history | Done | Real persisted history and daily/monthly transfer stats |
| Installer/release | Done for private use | Portable zip, IExpress per-user installer, shortcuts, Add/Remove Programs entry, uninstaller, `v1.0.1` |
| Verification | Baseline done | 64 tests, mock NNTP, golden screenshots, installer/packaging checks, off-screen tray/download/shutdown scripts |
| Code signing | Optional | No certificate configured |
| Live service validation | Blocked | Needs a second server |

### Fixed: every download was byte-corrupted (2026-11-04)

Reported by the user: RARs downloaded by NewsQueue fail extraction while the same
NZB downloaded by IDM is fine. Confirmed and fixed.

- **Cause.** `decode_yenc` (`app/src/download.rs`) decoded an escaped byte as
  `byte - 64` instead of `byte - 64 - 42`. yEnc stores `(b + 42)` and escapes
  that value as `'='` plus `(value + 64)`, so the unescape step must subtract
  106. Every byte whose encoded form needed escaping — roughly 1.6% of a random
  payload — was written 42 too high, so archives passed their length and
  progress checks but failed CRC/extraction. The bug has existed since the first
  commit (`b81a2c9`), so **no NewsQueue download before this fix can be trusted**,
  including anything from the `v1.0.0` release.
- **Evidence.** `how-f4247f.nzb` output vs the same NZB downloaded by the
  original AltBinz: first difference at offset 10 of `part1.rar`, `0x0D` instead
  of `0xE3`; 8,223,382 differing bytes in `part1.rar`, and all of them satisfy
  `corrupt == (valid + 42) mod 256` for a value in the escaped set. Nothing else
  differed, which rules out holes, truncation, dot-unstuffing and yEnc part
  offsets.
- **Fix.** Subtract both offsets in `decode_yenc`, plus regression tests:
  `decodes_yenc_escapes` (`=}`, `=@`, `=M`, `=J` and a plain `*`) and
  `downloads_binary_articles_with_yenc_escapes`, which round-trips all 256 byte
  values through the mock NNTP server. The existing mock download test only used
  the ASCII bytes `A`–`J`, whose encoded forms are never escaped, which is why
  the bug survived the suite.
- **Verification.** The fixed build re-downloaded `how-f4247f.nzb` and all six
  volumes are SHA256-identical to the good reference copy (`part1`–`part5`
  524,288,000 bytes, `part6` 376,364,335 bytes, `ALL MATCH`). `UnRAR t` over the
  set reports `All OK`, exit 0. Before the fix the same NZB produced archives
  that unpacked to garbage.
- **Still to do.** Add a checksum-on-complete or `=yend size` cross-check so a
  decode bug cannot pass on length alone again.


### Added: archives without extensions (obfuscated posts, 2026-11-04)

Obfuscated releases arrive as `Rar!` payloads with random names such as
`1xnkjchunrcod4zjpbnri` and no extension, so `find_archive` (which matched
`.rar`/`.001`) found nothing and Unpack reported "No RAR archive found".

- **Detection.** New `app/src/container.rs` sniffs the decoded payload for RAR5,
  RAR4, ZIP, 7Z and PAR2 magic bytes and reads the RAR5 volume number from the
  main archive header.
- **Naming.** A download whose chosen name has no extension is renamed from its
  content: RAR5 volumes become `<collection>.partNNN.rar` using the header
  volume number, so UnRAR can chain them; single-file ZIP/7Z/PAR2/RAR downloads
  get their real extension. Other multi-file formats keep the post's own name so
  volumes cannot overwrite each other.
- **Repair on unpack.** `unrar::normalize_obfuscated_names` runs before
  `has_archive`/`extract`, so folders already on disk are renamed in place and
  can be unpacked without re-downloading.
- **Verified.** `jada-39768c` (55 RAR5 volumes, 11.4 GB mp4, random names) is
  detected, renamed `jada-39768c.part001..part055.rar`, listed by UnRAR as
  `RAR 5, volume 1` and passes `UnRAR t` with `All OK`.

### Phase summary

- **Phase 0 - done for the baseline.** 64 tests, mock NNTP tests, screenshot
  checks, a golden tab/dialog screenshot set, real damaged-RAR/PAR2 fixtures,
  an encrypted-RAR fixture and the user-provided NZB fixtures exist.
- **Phase 1 — done for the core path.** Missing articles no longer abort a job;
  resume sidecars (`.nqpart`) preserve downloaded articles across restarts; all
  enabled servers are used; watched-folder auto-import, retry/backoff, thread
  priority, article cache and execute hooks work; mock NNTP tests cover success,
  cache reuse, missing-article completion, 430/CRC failover to another server,
  reconnect-on-retry and duplicate-name handling. Live failover still needs a
  second server.
- **Phase 2 — done for the core chain.** `app/tools/par2.exe` verifies and
  repairs real sets. A damaged RAR is repaired and then extracted in tests.
  Download → PAR2 → unrar is wired through `App::poll_par2`.
- **Phase 3 — done for the common path.** Overwrite/rename, strip-root, password,
  delete archives, move/delete patterns, target folder, pause-while-unraring,
  manual unpack, auto-rename to collection, delete-empty-folder, archive/group/NZB
  subfolders and the password prompt with retry work. Encrypted archives are
  detected, prompted for and retried with the entered per-collection password.
- **Phase 4 — RSS is implemented; search was cut.** Feed URLs and filters are
  stored locally (never committed); a live feed returned 100 items and the
  filter test downloaded one NZB. Global/Newznab search was removed on
  2026-11-18 and is out of scope.
- **Phase 5 — removed.** The Explorer tab was cut on 2026-11-18 (`REMOVED.md`).
- **Phase 6 — mostly done.** Log retention, persistent real history and transfer
  statistics, multi-server use, thread priority, 
  execute hooks and HTTP/SOCKS5 proxy support are implemented.
  Native tray show/restore/Exit, off-screen minimize-to-tray with taskbar removal,
  close-to-tray, speed tooltip and download/unrar balloon hints work. Windows
  shutdown, hibernate, sleep and app-exit actions run after the queue drains with
  a 15-second abortable countdown and a mockable backend. A few shortcut/polish
  items remain optional.
- **Phase 7 — done for private use.** Original icon, window/exe resources, a
  portable release zip (`app/package.ps1`), a golden screenshot matrix
  (`app/screenshots.ps1`) and a per-user IExpress installer
  (`app/build-installer.ps1`, tested by `app/installer-test.ps1`) are done.
  Code signing remains optional.

Live check on 2026-11-18 (single server, after the CRC and failover changes):
`Love.Under.the.Mistletoe` (1.21 GB, 5 files) downloaded with 0 missing articles
and PAR2 verified it clean. `Civilization.IV` (1.63 GB, 40 files, `.rar`/`.rNN`)
downloaded with 0 missing articles; PAR2 reported damage and repaired it, then
UnRAR extracted `rld-civ4.iso` (1.45 GB) without errors. The "repair" was not
damage: a verify-only run on a fresh download shows all 2270 RAR/SFV data
blocks intact and only `reloaded.nfo` missing. The NZB's 40 files do not include
that NFO, but the PAR2 set covers it, so PAR2 rebuilt it from one recovery
block. The downloader output was byte-exact.

Current state: usable Windows clone. Strong on layout, queue/status maths, NNTP,
PAR2, unrar extraction, tray/shutdown behaviour and version check/self-update.
Remaining work is external validation, a little mock/failover coverage and
optional polish.

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
  folder, archive/group/NZB subfolders, both delete-archive flags, delete-after-unpack
  and move-to-unrar patterns.
- Prompt for passwords; store per-collection passwords and retry extraction after
  the entered password is supplied.
- Trigger after PAR2, from the PAR2 tab, and from the queue context menu.
- Handle cancellation, existing files, multi-volume gaps and failure cleanup.
- **Done when:** each option has an automated or manual test and `Unpack now`
  works on a real downloaded set.

## Phase 4 — RSS (L)

- Real RSS/Atom fetching and refresh timer.
- Implement the original filter grammar (`| A/D/F/P/R`, download/unrar folders,
  execute command) and autodownload decisions.
- **Done when:** a feed fixture produces the expected accept/deny decisions.

## Phase 5 — Explorer shell parity (removed)

Cut on 2026-11-18; see `REMOVED.md`.

## Phase 6 — settings, history and auxiliary features (M)

- Settings are enforced for proxy, thread priorities, auto-import,
  execute, log retention and history.
- Download history and daily/monthly transfer statistics are real and persisted
  in `newsqueue-history.json` / `newsqueue-stats.json`; demo values only appear
  in screenshot mode.
- Windows shutdown/hibernate/sleep/exit runs after a 15-second abortable countdown;
  the mock backend keeps the power calls testable.
- **Done when:** every visible setting has an observable effect and a test.

## Phase 7 — visual and release polish (M)

- Screenshot diffs for every tab/dialog; fix layout drift.
- Error dialogs and recovery paths matching the original where practical.
- Portable release zip, per-user installer and release build are done; code
  signing remains optional.
- **Done when:** the parity checklist is green on a clean Windows machine.

## What is still needed

### Blocked on test data

1. **Done: live download → PAR2 → unrar.** Both supplied NZBs downloaded
   cleanly; Mistletoe verified, Civilization.IV was repaired by PAR2 and
   extracted; its only PAR2 "damage" was an NFO the NZB never listed.
2. **A second Usenet server account.** Multi-server scheduling is implemented,
   but live failover needs a second account. Mock 430/CRC failover and
   reconnect tests cover the logic.

### Blocked on external services

3. **Code-signing certificate** if signed release binaries are required. This is
   optional for private use.

### Not blocked

5. **Downloads are verified per article** by the yEnc `pcrc32` (or `crc32`
   for single-part posts); a mismatch is fetched from another server.
6. **Mock NNTP read-timeout test** is still missing (failover, 430, CRC and
   dropped-connection paths are covered).
7. **Self-update needs public releases.** The check works but the repo is
   private, so GitHub answers 404. Only releases built after 2026-11-18 can
   self-update.
8. **Minor shortcut polish** and any remaining peripheral settings.
9. **Per-setting verification checklist**; the current baseline is tests,
   screenshots and focused scripts.

## Installer: Inno Setup 7 (planned)

Replace the IExpress setup with Inno Setup 7 (installed on the build machine
at `C:\Program Files\Inno Setup 7`, compiler `ISCC.exe`). Not started.

- `app/installer/newsqueue.iss` compiled by `build-installer.ps1`; same output
  names (`NewsQueue-<version>-Setup.exe` + `.sha256`), same release assets.
- Per-user, no admin: `PrivilegesRequired=lowest`, default dir
  `{localappdata}\Programs\NewsQueue`, Start Menu + optional Desktop shortcut,
  real uninstaller; settings/queue/history beside the exe are never deleted
  on reinstall or update.
- Fixed `AppId` (Inno writes `<AppId>_is1` under the Uninstall key, not
  `NewsQueue`): on first install remove the old IExpress `NewsQueue` entry and
  shortcuts so Apps & features shows one item.
- Self-update: `update.rs` runs the setup with `/VERYSILENT
  /SUPPRESSMSGBOXES /NORESTART` (+ `/DIR=` of the running install); keep the
  wait-for-PID and relaunch behaviour (`NQ_WAIT_PID`/`NQ_RELAUNCH`, or
  `CloseApplications` + `[Run]`), so 1.0.2+ copies still update themselves.
- Rewrite `installer-test.ps1` for the new key/flags; also cover an update
  over an existing IExpress install.
- Optional later: signing (`ISSigTool` / code signing).

## Planned features (future)

Features common in SABnzbd/NZBGet that NewsQueue lacks, in priority order.
None are started. Categories stay cut (see `REMOVED.md`).

### High value

1. **On-demand PAR2.** Download the small index `.par2` first and hold back
   the `.volNN+NN.par2` recovery files; after verify, fetch only as many
   recovery blocks as the damage needs. Saves the 5–15% of each job that is
   usually unused recovery data.
2. **Archive password from the NZB.** Read `<meta type="password">` in the
   NZB head and the `{{password}}` suffix in the NZB file name; store it on
   the collection so unrar never prompts for it.
3. **7z and ZIP extraction.** `container.rs` already detects both; add an
   extractor (vendored `7z.exe` on Windows, `7zip` on Linux) beside UnRAR.
4. **Abort hopeless jobs and pre-check.** Stop a job once missing articles
   exceed the available PAR2 recovery blocks, and optionally `STAT` articles
   before downloading to reject incomplete posts early.
5. **Direct unpack.** Start UnRAR on a multi-volume set while later volumes
   are still downloading, so extraction finishes soon after the download.

### Medium value

6. **Duplicate detection.** Warn or skip when an NZB name or title matches
   an item in history.
7. **Post-success cleanup.** After a clean extract, delete `.par2`, `.sfv`,
   `.nzb`, `.nfo` and sample files (each optional, configurable patterns).
8. **Retry from History.** History keeps enough to re-queue a failed or
   incomplete job with one click.
9. **Notifications.** Webhook, ntfy/Pushover and email on completed/failed
   jobs; needed once the engine runs headless in Docker.
10. **Per-server quota and retention.** A data cap per server and skipping a
    server for posts older than its retention.

### With the Docker/web work

11. **Web API with authentication** (GUI_PLAN Phase 1), which also enables
    phone control and scripting.
12. **Pattern-based move/rename** of finished output (a small, category-free
    version of SABnzbd sorting).

## Suggested next order

1. Add a mock NNTP read-timeout test.
2. Optional: report files a PAR2 set covers but the NZB does not list as
   "not in NZB" rather than as a generic repair.
3. Exercise live second-server failover with a second account.
4. Optional: code signing, shortcut polish and a full
   per-setting checklist.

Download robustness, PAR2 repair, unrar, tray and shutdown are already in place.
The remaining gaps need external data or are optional polish.
