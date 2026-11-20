# Native Compact polish audit — 2026-11-20

Production candidate: `target/newsqueue-polished.exe` (optimized Windows GUI
build, no console). Required PAR2/UnRAR tools and licenses are beside it in
`target/tools`. No installed copy, real settings, queue, or credentials were
changed. The Qt6 side prototype and egui web frontend were outside this pass.

## Corrections

- Statistics tab and chart handles now participate in the dialog control
  registry. All five tabs select correctly; History and Miscellaneous hide
  the chart. Their controls inherit the native tab-pane background, removing
  gray rectangles behind text.
- The main native toolbar wraps at narrow widths instead of losing its final
  commands. The RSS filter/count move to a second row when needed.
- Status panes adapt to window width and expose their full text as tooltips.
- The resizable RSS editor now lays out both text areas, labels, separator,
  and buttons on resize and enforces its minimum client size.
- Notes and Setup explanatory text use measured wrapped font height, fixing
  clipping at 150% scaling. Password prompts wrap long collection names;
  prompt controls and footer move down to make room.
- Dialog gray/accent tags are removed on destruction, preventing reused HWNDs
  from inheriting a previous control's text color.
- The Setup scheduler grid is registered, so Allow all / Block all invalidate
  the actual grid instead of passing a null window handle.
- The approved menu-loop responsiveness change is retained.

## Reproducible matrix

From the repository root:

```powershell
$env:RUSTC_WRAPPER = ''
cargo rustc --release --locked --offline -p newsqueue --bin newsqueue --manifest-path app/Cargo.toml -- -o C:/Users/kaank/Desktop/news/app/target/newsqueue-polished.exe
powershell -NoProfile -ExecutionPolicy Bypass -File app/ui-polish-audit.ps1 -Executable C:/Users/kaank/Desktop/news/app/target/newsqueue-polished.exe
```

The script runs with sample data on unshown-owner windows outside the virtual
desktop. It injects no keyboard/mouse input, changes no system settings, starts
no engine/tray/network thread and restores its environment variables.

129 captures at each of 96 / 120 / 144 / 192 DPI (100 / 125 / 150 / 200%):

| Surface | Cases per DPI |
| --- | ---: |
| Main: four tabs × three nominal sizes × normal/empty-queue/long-name states × connections shown/hidden | 72 |
| All Setup pages | 15 |
| Server, Statistics, Notes, About, Columns, Pause, Speed, Version, Shutdown, Password, Rename, Execute, RSS feeds | 13 |
| Four further Statistics tabs | 4 |
| All ten speed choices | 10 |
| All four idle shutdown methods | 4 |
| Active pause, long password prompt, enlarged RSS editor | 3 |
| Add/edit server × TLS on/off × authentication on/off | 8 |
| **Total** | **129** |

Main sizes are nominal outer-window 760×480, 1024×700 and 1280×820 logical
units. Windows can constrain oversized windows to its display/work-area
limits; captures and measured child bounds record the actual resulting size.

## Observed results

- Final optimized build: **516 captures**, all four processes exit 0.
- Each capture has an inventory of visible app controls with parent-relative
  bounds and measured static-text height: **zero detected layout warnings**.
- Reviewed contact sheets covering the app's surfaces, plus full-size images
  of narrow RSS/main layouts, Statistics History/Miscellaneous and high-DPI
  Setup text. Earlier captures exposed the corrections above.
- Final optimized `--ui-smoke` passed at all four DPI settings. It verifies
  all five Statistics tab selections/visibility, RSS resizing/minimum size,
  color-tag lifetime, every toolbar item within bounds at three widths,
  RSS filter/count non-overlap, filtering, pause/resume dispatch, menu-loop
  timer isolation/resumption, all Setup pages/dialog constructions and real
  shell-dialog COM factories. It does not display shell dialogs.
- Workspace tests: **118 passed** (25 desktop, 80 core, 12 server, 1 web).
  The sandbox denied local mock-server connections with 10013; the permitted
  loopback-enabled rerun passed. `git diff --check` passed.
- An intermediate text-measurement change failed on an empty hint. The empty
  string is now handled before calling DrawText, and prompt construction is
  included in the expanded smoke and final matrix.

Final evidence: `target/polish-audit/release-verified/dpi-{96,120,144,192}/`,
including PNGs, `controls.json`, `issues.json`, and visual contact sheets.
Smoke logs are `target/polish-audit/release-smoke-*.out`; workspace results are
in `target/polish-audit/tests.log`. Earlier failed/intermediate captures remain
separate; they are not the acceptance evidence. Existing goldens are unchanged.

This is a broad, finite visual/state matrix, not a claim to test every possible
settings combination or arbitrary input. Real monitor transitions, alternate
Windows themes/versions, live pointer/keyboard/tray interactions, shell-picker
and message-box visuals, network error permutations, installation and release
publishing were not exercised. Those require separate scopes; this pass kept
the user's desktop untouched.

## Scrolling and folder memory (2026-11-20)

`--ui-scroll-smoke` uses only off-screen native demo controls, with 500 rows in queue/connections/PAR2/RSS and 150 log rows. At 96 and 192 DPI it verified 60 wheel steps, vertical/horizontal movement, timer isolation, preserved viewport after selection restoration, and log reading/tail-follow. Maximum observed wheel dispatch was under 4 ms on the pre-release build; this is not an end-to-end input latency measurement. Real mouse/touchpad gestures and scrollbar dragging remain untested under the no-desktop-control preference.

Open-file pickers save their last successful parent folder by dialog title in the gitignored `newsqueue-picker-folders.json` beside application state. Cancel does not overwrite it; stale paths are ignored. Unit coverage includes reload, multiselect, separate picker histories, overwrite, cancel, missing folders and malformed history. The real shell factory regression also checks `SetFolder`/`GetFolder` without showing the dialog.

## Release 1.0.8 verification

The packaged executable passed `--ui-scroll-smoke` and `--ui-smoke` at 96/192 DPI. Maximum wheel dispatch was 2.09/4.02 ms respectively. All 119 workspace tests passed (26 desktop, 80 core, 12 server, 1 web). The current egui WASM build succeeded and is included. `installer-test.ps1 -Hidden` passed silent install/reinstall, old-process wait, isolated registry/shortcuts, installed off-screen screenshot, uninstall, and preservation of settings/folder history. The legacy visible wizard/relaunch path was deliberately skipped. The portable archive contains only the executable, tools/licenses, icon, README/roadmap and web assets; no runtime state or downloaded files.


## Approved QoL follow-up — 2026-11-20 (local build)

- Shared main toolbar order starts Connect, Add NZB, Pause, Pause for, Speed,
  then movement, Unpack/Delete, Clean up, Stats, Options. Desktop Version button
  and direct Help update entry removed; About contains Check for updates.
- Native Segoe UI has a 14 px minimum at 96 DPI, scaled with DPI. Label heights
  are measured; longer captions shortened to prevent clipping. Native connections
  divider can use available space while leaving a usable queue area.
- Web Sea Glass/Copper use bundled OFL Inter (14 px body); Classic typography is
  retained. Connections divider supports dragging, remembered browser height,
  double-click reset and arrow-key adjustment. Settings categories switch during
  primary-button dragging; ordinary hover leaves the page unchanged. TLS toggles
  default 119/563 ports only. Text fields retain the I-beam, other surfaces the arrow.
- Shared custom caps are decimal MB/s; invalid/nonfinite inputs are rejected.
  New and active workers use the cap. Throttle waits recheck changes and stop/pause
  every 25 ms, so removing a very low cap does not leave a worker asleep.
- Temp-folder descriptions corrected: it is reserved/unused; payloads and resume
  sidecars remain in each job's download folder.

Verification:
- 136 workspace tests (26 native, 82 core, 14 API, 14 web) passed, including new
  custom-cap validation/mode switching/API publication and interruptible throttling.
- `target/qol-audit-final/dpi-{96,120,144,192}`: 129 captures per DPI, 516 total,
  zero measured parent-boundary/text-height warnings. Covers 15 Setup pages,
  dialogs, server TLS/auth combinations and main-window size/tab/data variants.
- Hidden native controls/toolbar/speed-parser and scrolling smoke passed at
  96/192 DPI; logs in `target/qol-audit-final/`.
- `web/qa/qol.py`: real egui browser actions against an isolated authenticated demo
  server; divider persistence, custom cap/No limit, TLS default/custom ports,
  category drag/no-hover navigation and cursor checks. All three styles in light
  and dark; all eight settings pages at 390, 760 and 2560 px widths. Evidence in
  `target/web-audit/qol-shots/`.
- No real Usenet download or production restart. These changes are not a new
  published release; existing deployed/installed versions remain separate.


### Windows font/divider correction — 2026-11-20

The user requested the original Windows typography. Restored the OS message font
and original shared label/note/memo geometry; larger Inter remains web-only.
Reproduced the reported divider defect in an offscreen regression: the toolbar
height grew from 54 to 56 pixels on the first drag layout. TB_SETROWS may return
the existing toolbar height when rows do not change, so adding padding each time
caused cumulative growth. Layout now derives height from actual button bounds.
The regression drives the real divider WM_MOUSEMOVE handler 80 times at each of
three window sizes, without capturing or moving the desktop mouse. Native smoke
and scrolling checks pass at 96/192 DPI. Prior larger-font captures are historical.

Local web review runs independently at http://127.0.0.1:18510 with isolated demo
state under target/qol-preview; it does not control the deployed Docker queue.

## 1.0.9 release verification

The Windows single-instance option is enabled by default, with an exclusive
per-profile lock before engine startup. A hidden two-process test exercises
Unicode NZB handoff through WM_COPYDATA, and tests cover lock release,
independent profiles, old-settings defaults and explicitly disabling the option.
The full workspace suite passes: 140 tests (30 desktop, 82 core, 14 server,
14 web), plus the spawned handoff helper. Diagnostic windows bypass the lock.

The release keeps the original Windows font and includes the repeated-divider
regression test. Desktop input/focus and tray interaction are not automated
against the user's live desktop; verification uses offscreen native windows and
headless browsers. The installer check uses its isolated test identity and
hidden mode, which skips the visible legacy wizard/relaunch check.

### Engine audit and independent re-review (2026-11-20)

Release was held at the user's request for an engine robustness/performance
review. The initial tests reproduced truncated yEnc acceptance, all-missing jobs
reported complete, and full-length partial files incorrectly skipped. Fixes add:

- Strict yEnc trailer sizes, inclusive part ranges and valid CRC text; 1–8 hex
  digits remain compatible with real posts that omit leading zeroes.
- Advertised whole-file CRC checking, contiguous verified ranges through the
  declared file size, and checksummed resume/complete receipts (`.nqpart` and
  `.nqverified`). Disk changes, orphaned/duplicate/legacy receipts and missing
  first articles cannot establish false completion.
- Reserved payload/metadata names, bounded NNTP response/article reads,
  malformed/overflowing NZB rejection and message-ID validation.
- All-provider failover with one pending article, transport/missing distinction,
  inconclusive STAT handling and interruptible retry waits.
- Coordinator ownership until cancellation drains, non-reused collection IDs,
  immediate active-delete retirement, and queue-drained shutdown worker gates.
- Verified volume readiness and deferred direct-unpack cleanup, atomic durable
  state replacement, and queue persistence before deleting auto-imported NZBs.
- Hardware-accelerated CRC via crc32fast, immediate first-part writes instead of
  retaining every first payload until preparation ends, bounded output handles,
  and sequential reused-buffer resume verification.

Astra independently reviewed the implementation and ran the 11 focused downloader
integrity tests, including its discovered missing-tail resume scenario. All
passed after correction; no remaining blocker was identified in the reviewed
resume/CRC interaction. This is targeted review, not a proof of absence of bugs.

Ghidra analysis of a workspace copy of the original AltBinz 0.48.4 cache confirms
size/range validation in local assembly worker `0x866c64` at
`0x8674fd–0x86763f`, following decoder call `0x867445`; CRC comparison is at
`0x867644–0x8676fd`. The original also retains previous queue index generations.
No evidence was established for re-hashing resumed payloads or exhaustive NNTP
provider failover. Generated evidence remains under
`app/target/ghidra-integrity-audit`; original evidence was not modified or run.

### Real-server performance evidence (initial measurements)

User-authorized Easynews tests use one complete 500 MiB file from a supplied NZB,
20 connections, isolated output/config folders, no unpacking and no UI activation.
Credentials and NZBs are not source/release artifacts. Every successful case below
produced the same SHA-256 payload. Intel Core i7-14700KF, Windows; CPU seconds are
process CPU time (not percent of the entire 28-thread machine).

| Case | TLS MB/s | Plain MB/s | TLS / plain CPU seconds | First payload |
|---|---:|---:|---:|---|
| Pre-audit CLI baseline | 45.4 | 54.9 | 3.81 / 3.63 | ~1.5 s / ~1.5 s |
| Audited CLI | 40.2 | 47.5 | 1.94 / 2.25 | ~1.5 s / ~1.0 s |
| Actual native Win32 controls + engine, hidden | 46.3 | 50.7 | 2.53 / 2.52 | 1.05 s / 0.84 s |

Throughput varied across sequential live-network runs; these are not a controlled
claim of a throughput increase. The audited CLI used less CPU while additionally
verifying disk contents. Native API response p95 was 17.9 ms TLS / 13.7 ms plain.
`--ui-background` is the isolated native QA mode: it creates real controls and
runs their refresh loop but suppresses visible windows, tray and event dialogs.
Visible-window painting/interaction is not covered by this performance test.
### Final release verification

The final runtime commit passed **156 workspace tests** (30 native, 98 core,
14 server, 14 web). Hidden native audit: 516 captures across 96/120/144/192 DPI,
zero layout warnings. Native control/scroll/divider smoke passed at 96/192 DPI.
Production WASM rendered with authenticated API access, bundled font licenses,
no audit hooks and no JavaScript errors. The silent isolated installer check
passed install, update, launch/screenshot, uninstall and user-data preservation.
The Linux image passed the CI Compose/container smoke check.

Final-runtime live-server measurements (same 500 MiB file, 20 connections):

| Frontend / host | TLS MB/s | Plain MB/s | TLS / plain CPU seconds | First payload TLS / plain |
|---|---:|---:|---:|---:|
| Hidden native Win32 / Windows i7-14700KF | 46.1 | 47.4 | 3.88 / 3.39 | 1.10 / 0.83 s |
| Headless server + web / same Windows PC | 41.7 | 48.1 | 4.00 / 3.95 | 0.99 / 0.80 s |
| Docker + web / Linux Intel N100 | 56.8 | 51.5 | 6.35 / 5.19 | 1.00 / 0.73 s |

All six final runs completed and produced SHA-256
`c663d350e925c53a812d71e93d8255a2e54798a69d3c6cdc791d4b3d83d2e0d3`.
The web browser ran headlessly on Windows during both server comparisons.
Docker API response p95 was 17.9/17.2 ms; browser CPU time was 1.47/1.00 s.
Docker cgroup peak memory includes filesystem page cache (611/1129 MiB across
successive downloads), so it is not comparable to process RSS. Windows native
peak RSS was 38.0/33.7 MiB and headless server RSS 32.8/24.5 MiB.

These measurements establish comparable download performance and identical
payloads, not exact throughput equality across different hardware or live-network
runs. There was no observed frontend throughput bottleneck. The initial CLI CPU
improvement was not uniform across final GUI/server runs; do not generalize it
into a guaranteed CPU reduction. Tests use isolated output folders, with no
production queue changes or visible UI interaction.

### Multi-file startup correction (2026-11-21)

The earlier single-file benchmark did not cover multi-file startup. The released
1.0.9 downloader fetched the first article of every file serially before starting
its workers, even with STAT disabled. The connection panel inferred activity from
queue state and worker count, so it falsely described preparation as downloading.

- Named NZB files now enter the worker queue immediately; no separate BODY name
  lookup is needed. The first valid article establishes the advertised size;
  inconsistent headers still fail validation and can fall back to another server.
- Extensionless/obfuscated files retain content/name discovery, but each prepared
  file starts streaming while later files are discovered. Discovery reserves one
  configured connection per provider, rather than exceeding connection limits.
  When every provider has just one slot, each prepared file drains before the next
  discovery. Deterministic naming and checksummed resume validation remain.
- Worker ownership survives preparation errors/cancellation. Pending production
  prevents a temporarily empty work queue from ending the download prematurely.
- Concurrent duplicate articles coalesce through the bounded cache. Valid resumed
  parts no longer require an extra first-article download just to establish names.
- The shared connection rows report actual preparation, connection, article,
  writing, verification, retry, waiting and failure states. Article rows include
  received encoded-body KB and elapsed time; collection progress still counts
  decoded written payload. Optional STAT shows article/provider counters and stops
  repeating commands against a broken or unsupported STAT connection.

Controlled regressions cover earlier payload streaming before a delayed later
lookup for named/obfuscated files, one/two connections, and two single-slot
providers. A separate test gates an article's final bytes on receiving an interim
progress callback. Existing integrity, provider fallback, resume, cache, proxy,
PAR2, extraction and direct-unpack checks remain enabled. 158 workspace tests
passed serially (30 native, 100 core, 14 server, 14 web); hidden native 96/192 DPI
and production WASM/authenticated API smoke checks passed. The extra two-provider
single-slot case also passed after the full suite.

Real account verification used the full user-supplied 59-file jada NZB, 8
connections, fresh isolated folders, STAT/cache/unpack/PAR2 disabled. Results:

| Build / transport | First written payload | First completed file | Entire job |
|---|---:|---:|---:|
| Released 1.0.9 / TLS | 1.85 s | 79.51 s | 158.56 s |
| Initial streaming fix / TLS | 0.79 s | 2.60 s | 68.45 s |
| Named-file fast path / TLS | 0.83 s | 1.79 s | 78.47 s |
| Named-file fast path / plain | 0.65 s | 2.16 s | 85.98 s |

All 59 SHA-256 hashes matched across all four complete 2,944,891,658-byte jobs.
Network throughput varied, so this demonstrates removal of the startup barrier,
not a guaranteed total-speed multiplier. No minute-long mid-file stall occurred
in these runs; its exact cause in the user's earlier session remains unproven.
The new worker telemetry makes such waits distinguishable. This aligns the
supported download paths with immediate streaming; it is not an exhaustive
proof of identical AltBinz behavior in every failure scenario. No new public
release or Docker deployment is implied by this local correction.

The installed Windows executable was replaced after validation, with the previous
binary retained in `app/target/newsqueue-installed-before-startup-fix.exe`.
Installed `--ui-smoke` passed without activation, and hashes of all existing
settings/queue/history/statistics/picker JSON files were unchanged. The desktop
shortcut still points to the same installation. Docker and public release assets
remain unchanged.
