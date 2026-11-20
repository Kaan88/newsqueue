# Web theme verification — 2026-11-20

Caustica is restored from the original Instrument/Caustica implementation in commit `6835951`, including IBM Plex fonts and their SIL OFL license. The web frontend remains egui/WASM. View selects Caustica or Classic independently of System/Light/Dark. Preferences persist; System follows browser color-scheme changes. New profiles default to Caustica.

## Polish

Checked color harmony, text contrast, field focus, disabled states, dialog headers, spacing, narrow layouts, scrolling and footer access. Caustica uses neutral surfaces and consistent teal accents. Light accent and warning text were darkened for contrast, scheduler greens softened, and Classic-only blue chart bevels removed from Caustica. Original Classic palette, typography and dimensions remain intact.

## Observed verification

- 133 workspace tests passed: 26 desktop, 80 core, 13 server, 14 web. Command: `cargo test --locked --offline --workspace -- --test-threads=1`, with process-local `RUSTC_WRAPPER` cleared.
- 864 visual captures: two themes, two brightness choices, three viewport sizes (1280×820, 760×600, 390×844), two pixel ratios, 36 surfaces each. Includes five tabs, five menus, seven dialogs, three Statistics pages and all eight Options pages at top/bottom.
- 60 Classic comparisons against the pre-change 1.0.8 WASM: 59 pixel-identical; one differs at three pixels by one RGB level on a disabled icon (raster rounding).
- 80 edge-state captures: empty lists, 500-row scrolling, long Unicode subjects, focused filtering, simulated outage and recovery across both themes/brightness choices and desktop/narrow widths.
- Final targeted follow-up covers main window, server editor, scheduler, both transfer charts and disabled menu hover after the last palette refinements.
- Real API browser flows passed in all four theme/brightness combinations: authentication, pause/resume, context menus, rename/reorder, archive password, expand/collapse, filtering, speed limit, timed pause, settings cancel/apply, multiple-file NZB import, delete cancel/confirm, preference persistence, live System preference and sign-out.
- Administrative flows passed in all four combinations: server add/edit/remove, credential retention, failed-save retry, RSS add/remove, scheduler changes, malformed NZB rejection and file-picker cancellation.
- Production WASM built without `ui-audit`; checked that `__nqAudit` and `__nqState` are absent.

## Functionality fixes discovered

The server editor could reload stale data before its asynchronous save was applied. It now waits for confirmed server data and retains a retryable draft after failure. The API now returns server indices so edits preserve stored credentials and hidden fields, including after removing a row. Regression tests use synthetic credentials and actual GET/POST round trips. The web client also supplies indices for older API replies.

## Evidence and limits

Local generated evidence is under `app/target/web-audit`: `final-matrix`, `states`, `postpass`, and `final-tests.log`. Reproduction harnesses are in `app/web/qa`.

All browser testing used hidden headless Chrome and isolated demo servers/configurations. No desktop input, installed-app replacement, live Usenet/RSS download, or real account changes. The high-DPI harness forces eframe's supported content-box-times-DPR path because Chrome emulation reports CSS pixels in its device-pixel content box. This is an emulation workaround, not a production change. Physical monitor transitions, actual touch devices, Firefox/Safari, and real network downloads were not exercised. This is a broad sampled matrix, not a claim to test every possible state. This audit preceded the server/web 1.0.8-web.1 release supplement; Windows 1.0.8 binaries are reused unchanged.
