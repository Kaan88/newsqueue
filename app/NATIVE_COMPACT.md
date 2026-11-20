# Native Compact desktop migration

Latest production candidate and follow-up verification: `target/newsqueue-polished.exe`.
See `UI_POLISH_AUDIT.md` for the 516-case hidden polish pass and corrections.

The user approved the polished Compact Win32 mockup on 2026-11-20 and asked to
use it in NewsQueue. This is applied to the production Rust desktop frontend,
not just the demo. The engine, server and egui web frontend are unchanged.

## Native implementation

- Common controls v6: ToolbarWindow32, SysTabControl32, SysListView32, Edit,
  Button, ComboBox, ListBox, up/down controls and the native status bar.
- Windows-managed title bars, captions, borders, menus and scrollbars.
- 30 px main icons / 46 px buttons; 16 px queue icons / 22 px buttons.
- Continuous system button-face command/filter rows, white edit surfaces,
  native gridlines, themed green/amber/error progress.
- All existing commands, tooltips, Setup pages and persisted sizing preferences
  remain connected to the production engine. Mockup data does not enter normal
  runs. The old classic styling module is compiled only in historical tests;
  production no longer patches imports, hooks captions or disables visual styles.
- System pickers retain cancellation/error handling and their scoped theme guard.

## Background verification

From `app`, with the process-local wrapper cleared only if needed:

```powershell
$env:RUSTC_WRAPPER = ''
cargo rustc --locked --offline -p newsqueue --bin newsqueue -- -o target/native-compact.exe
$exe = (Resolve-Path target/native-compact.exe).Path
$p = Start-Process $exe -ArgumentList '--ui-smoke' -WindowStyle Hidden -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Native smoke failed: $($p.ExitCode)" }
cargo test --locked --offline --workspace
```

The alternate output path avoids replacing an existing running debug binary.
`--ui-smoke` uses real window procedures and common controls with a sample
engine, and has no engine thread, tray, network connections, state persistence,
input injection or activation. Its window has an unshown owner and lives
outside the virtual desktop, so it does not appear on the taskbar.

Observed on 2026-11-20:

- Native build passed.
- `--ui-smoke` passed: gridlines, filter results, four tabs, collection
  pause/resume through WM_COMMAND and EngineHandle, menu state updates, resizing,
  all 15 Setup page constructions, auxiliary dialogs, real COM picker factories.
- Workspace tests passed: 25 desktop + 80 core + 12 server + 1 web = 118.
  Tests ran with permission for their local loopback mock servers.
- 34 off-screen captures passed: queue at forced 96/120/144 layout DPI;
  all 15 Setup pages; server, stats, notes, about, columns, pause, speed, version,
  shutdown, password, rename, execute and RSS dialogs; PAR2/log/RSS tabs.
- Inspected queue/DPI and Setup captures plus all-page contact sheets for
  layout, text, system-color consistency and clipped controls.
- `git diff --check` passed. Existing screenshot goldens were not overwritten.

Artifacts live in ignored `target/native-compact-shots/`. `--screenshot` now
also renders off-screen without activation/taskbar presence. It retains the
existing NQ_DEMO_DIALOG, NQ_DEMO_TAB, NQ_DEMO_SETUP_PAGE and NQ_DPI selectors.

The earlier picker interaction run is recorded in PICKER_SMOKE.md. It predates
this styling migration; the post-migration picker check activates the real
factories but does not interact with the dialogs. Actual keyboard, tray and
per-monitor DPI transitions, other Windows versions and installed-release
packaging remain unverified. Forced-DPI captures are not a substitute for real
monitor-transition testing. User preference prohibits taking desktop control.

The installed app and real settings/queue were not replaced. The parallel
Qt6 request is isolated under `qt6-mockup`; it is not the production frontend.

## Context-menu follow-up

After a report of hover selection updating only on click, queue context-menu
tracking is now posted to the main message loop so the ListView can finish
right-button processing first. Any remaining thread-local mouse capture is
released before TrackPopupMenu; splitter state is cleared on capture loss.
Unhandled context menus return to DefWindowProc for standard handling.

`target/native-compact-menu-fix.exe` builds successfully and passes the hidden
UI smoke, including deferred-menu message dispatch and capture-loss cleanup.
The smoke does not move the pointer, acquire mouse capture, or open a popup.
Actual hover behavior has not been reproduced or verified under the user's
no-desktop-control preference; capture interference is the suspected cause.

The user subsequently confirmed that the capture patch did **not** resolve it:
highlighting waits until the pointer rests, in both context and menu-bar
dropdowns. The capture hypothesis is not an established cause.

The next build, `target/native-compact-menu-responsive.exe`, defers routine UI
refresh and synchronous engine/event work while Windows is tracking any menu,
then schedules a refresh on exit. The engine thread keeps running, and the
shutdown countdown still ticks. Hidden smoke verifies both kinds of menu loop
with engine access deliberately unavailable, and verifies refresh resumes.
Build, smoke, and diff whitespace checks pass. The user subsequently accepted
this change ("great") and requested the full polish pass. Automated physical
pointer hover remains untested under the no-desktop-control preference.

Debug builds buffer up to 4096 menu input/selection timing events and append
them to `menu-hover.log` beside the executable on menu exit. The thread-local
WH_MSGFILTER hook is passive and removed on exit; it never injects input or
changes settings. Entries contain relative microseconds and numeric message
state only. Release builds omit this diagnostic. This allows a subsequent
user-run reproduction to distinguish delayed input from delayed selection.
