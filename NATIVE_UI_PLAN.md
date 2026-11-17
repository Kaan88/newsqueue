# Native Win32 UI plan

Status: proposal (2026-11-18). Nothing here is implemented yet.

## Recommendation

**Do Phase 0 (separate the core from the UI) first, whatever the final toolkit
turns out to be.** Then build a native Win32 spike of the main window and queue
list, and compare it with `shots/golden/queue.png` before committing to the full
port.

Why native Win32:

- AltBinz is a Delphi VCL app, and VCL is a thin layer over the Win32 common
  controls. Native ListView/TreeView, toolbar, tab control and status bar recreate
  it more faithfully than any custom drawing, which is the point of this proof of
  concept.
- Real ClearType text, per-monitor DPI, system theming, keyboard navigation and
  accessibility come for free.
- **The off-screen tray hack goes away.** eframe stops calling `tick` when its
  window is hidden, which is why `tray.rs` keeps the window visible off-screen
  and deletes its taskbar tab. A Win32 message loop keeps running while the window
  is hidden, so `SW_HIDE` to the tray plus a `WM_TIMER` tick just works.
- A virtual ListView (`LVS_OWNERDATA`) handles queues of any size without
  per-frame layout.

Costs: `ui.rs` (~2.2k lines) and `dialogs.rs` (~2.4k lines, 17 setup pages and
about 20 dialogs) get rewritten. There is no headless render, so screenshot
tests move to `PrintWindow` captures of a real window. Plan for about 3–5 weeks
of focused work in total, with Phase 0 about 1 week of it.

Crate choice: **`windows` (windows-rs)** for direct API access, with a small set
of in-house wrappers. `winsafe` is the alternative if raw `windows-rs` turns out
too verbose; decide during the spike. `native-windows-gui` is unmaintained, so
avoid it.

## Phase 0: separate the core from the UI (toolkit-independent)

Today `App` in `main.rs` holds everything: settings, queue, workers, timers,
logs, statistics and about 40 `show_*` dialog flags, and `App::tick` runs the
scheduler, workers, autosave and hooks in between UI frames.

Goal: a `core` module that knows nothing about egui or Win32.

1. Create `app/src/core/` with an `Engine` struct holding `settings`, `queue`,
   `history`, `stats`, the active download/PAR2/unrar workers, the log and the
   shutdown countdown.
2. Move the non-UI parts of `App::tick` into `Engine::tick(now)`. It returns a
   list of `Event`s (`QueueChanged`, `LogLine`, `Toast`, `PasswordNeeded(id)`,
   `ShutdownCountdown(secs)`, `Balloon`, …) instead of setting UI flags.
3. Replace UI mutations of `App` with `Engine::apply(Command)`, built from the
   existing `UiAction` enum (start, pause, move, delete, rename, import NZB,
   change settings, submit password, …).
4. Keep egui as the only front end during this phase. `App` becomes a thin shell:
   it holds `Engine` plus view state (selection, scroll, dialog drafts), calls
   `tick`, reacts to events and draws.
5. Move the integration tests in `main.rs` that do not need UI state to run
   against `Engine` directly.

Done when: `cargo test` passes, the golden screenshots are unchanged, and
`grep egui app/src/core` finds nothing.

## Phase 1: Win32 spike (main window and queue)

Behind a `--native` flag (or a `native` cargo feature) so the egui build keeps
working.

- Main window: rebar/toolbar with the 18 toolbar buttons and their icons,
  a tab control (Download queue, PAR2, Explorer, Log, RSS), the queue secondary
  toolbar and a status bar with the 8 measured panes.
- Queue: virtual ListView (`LVS_REPORT | LVS_OWNERDATA`) with the measured
  column widths from `QUEUE_COLUMN_WIDTHS`. Collections and their files shown by
  indenting rows with an expand glyph, which is how AltBinz draws them. Progress
  is custom-drawn in the Status column (`NM_CUSTOMDRAW`).
- Bottom pane: action list ListView.
- Tick: `SetTimer` at 100–250 ms calls `Engine::tick`; events invalidate only the
  affected rows (`ListView_RedrawItems`).
- DPI: per-monitor v2 manifest; scale the measured metrics from 96 dpi.

Exit criteria: download a mock-NNTP collection end to end in the native window,
and capture a `PrintWindow` screenshot next to `shots/golden/queue.png`. Decide
go or no-go on the full port from that comparison.

## Phase 2: tabs

- **PAR2**: report ListView.
- **Explorer**: TreeView plus ListView with the shell icon list
  (`SHGetFileInfo`), back/forward/up, favourites, drag and drop
  (`IDropTarget`/`DoDragDrop`) and clipboard (`CF_HDROP`).
- **Log**: virtual ListView.
- **RSS**: feed ListView plus item ListView.

## Phase 3: dialogs

- Build every dialog with a small layout helper that places controls at the
  coordinates measured in the original DFM evidence (`investigation.cany/`,
  `reference/`). The original layout is fixed-position, so this matches it
  exactly and scales with DPI.
- Setup: a modeless dialog with the page list on the left and 17 child pages,
  in the original order. Each page binds controls to `Settings` fields through
  a small `bind_check`/`bind_edit`/`bind_combo` table instead of hand-written
  message handling.
- The rest: server editor, categories, stats (custom-drawn bars and speed chart),
  search, columns, rename, pause, unrar password, shutdown countdown, split/join,
  execute command, RSS feeds, speed limit, notes, about, filter help.
- Common file and folder pickers via `IFileOpenDialog` (replaces `rfd`).

## Phase 4: tray, shutdown and cleanup

- Tray: keep the `Shell_NotifyIconW` code, but hide with `SW_HIDE` and drop the
  off-screen placement and `ITaskbarList3` tab removal.
- Rewrite `tray-verify.ps1` and `download-while-tray-verify.ps1` for the
  hidden-window path.
- Rewrite `screenshots.ps1` to launch with `NQ_DEMO_*` variables and capture with
  `PrintWindow`, then regenerate `shots/golden/`.
- Remove eframe, egui and egui_extras once the native build matches the golden
  screenshots, and update CLAUDE.md, AGENTS.md and ROADMAP.md.

## Risks

- **Effort on the setup pages.** It's mostly mechanical; the binding table keeps
  it manageable.
- **Owner-data ListView with a tree.** AltBinz fakes the tree with indentation,
  so this is a known pattern, but expand/collapse and selection need care.
- **Screenshot tests become flakier** with real windows (font smoothing, DPI).
  Compare with a tolerance rather than exact pixels.
- **Windows only**, which the app already is.

## Fallback

If the spike doesn't clearly beat egui, keep egui and do the cheap polish
instead: Segoe UI at the system size, DPI-correct `pixels_per_point`, crisp icons
at 16 and 24 px, toolbar hover and pressed states, and accent-coloured selection
and progress bars. Phase 0 still pays off either way.
