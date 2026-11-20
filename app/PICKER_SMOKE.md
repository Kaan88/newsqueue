# Native picker smoke test

Use this alongside `cargo test --locked --offline -p newsqueue pick::tests`.
The automated tests activate the real COM factories without showing windows.
This manual smoke exercises the actual window commands, native pickers and
engine delivery; it does not replace the picker with a mock.

Keep all agent-driven launches hidden and do not take desktop focus/control.
Run the interactive steps below only when the user explicitly requests them.
Preparation and artifact verification do not launch or control any window.

## Prepare (from app)

Build the candidate. If an existing `target/debug/newsqueue.exe` is in use,
build a distinct executable without terminating that process:

```powershell
$env:RUSTC_WRAPPER = '' # process-local only, if the configured wrapper is blocked
cargo rustc --locked --offline -p newsqueue --bin newsqueue -- -o target/picker-smoke-build.exe
.\picker-smoke.ps1 -Exe .\target\picker-smoke-build.exe
```

The script creates a fresh folder under `target`, copies the candidate, and
writes synthetic fixtures and credential-free settings with no configured
servers, feeds, auto-import, startup connection, update checks or Web access.
It refuses to overwrite an existing folder and launches nothing.

## Interactive checklist (explicitly requested sessions only)

Launch the prepared executable directly with `--config <prepared-folder>`.
Check its full executable path; do not use a launcher that resolves only the
`newsqueue.exe` basename. Never operate the installed application by accident.

1. Toolbar NZB: import `toolbar.nzb`; observe the Queued collection and file.
2. Ctrl+O: cancel; observe the same single collection and no error dialog.
3. File > Import: import `menu.nzb`; observe the second collection.
4. Ctrl+Shift+O: import `paused.nzb`; observe its Paused state.
5. Ctrl+O: select `multi-one.nzb` and `multi-two.nzb` together; observe both.
6. Import `invalid.nzb`; observe rejection in Log and no sixth collection.
7. Close and reopen using the same isolated config; verify all five collections,
   their segment-bearing files, and the paused state survive reload.
8. Select `toolbar`, Ctrl+E: save `exported-toolbar.nzb` into the prepared folder.
   Reopen Export and cancel; verify the queue is unchanged.
9. Select `toolbar`, Change the unrar folder: choose the prepared `download`
   directory. Close normally and check exit code 0.

```powershell
.\picker-smoke.ps1 -Mode Verify -Directory <prepared-folder>
```

Verify checks persisted state, export XML and the invalid-file log. It cannot
prove the interaction routes or cancellation: record those observations.

## Observed result, 2026-11-20

Before the fix: isolated debug binary, toolbar / File > Import / Ctrl+O each
returned without a picker. Instrumentation recorded `Create open dialog:
ClassFactory cannot supply requested class (0x80040111)`. A real COM probe
succeeded before styling, failed immediately after `SetThemeAppProperties(0)`,
and still failed after import patching and caption hooks. Thus the global theme
policy alone reproduced the failure; other hooks were not needed to trigger it.

After the fix: all nine steps above passed on the local fixed build. The live
window showed five collections, `paused` displayed Paused, and those rows
survived reopening. Export XML retained `toolbar@example.invalid`. Folder
selection persisted under the isolated tree. The final process exited 0.
Artifact verification passed against `target/import-audit`. Two earlier test
window closures were confirmed by the user as manual closures, not crashes.

`cargo test --locked --offline --workspace`: 118 passed (25 desktop, 80 core,
12 server, 1 web). The initial sandbox run denied loopback sockets (10013);
the permitted rerun passed. No live Usenet connection was made. The installed
application was not modified or tested. DPI/other-Windows-version coverage and
release packaging remain unverified.
