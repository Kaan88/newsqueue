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
