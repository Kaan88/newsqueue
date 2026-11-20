# NewsQueue native Win32 mockups

This standalone prototype keeps the NewsQueue menu, toolbars, tab row,
download queue, lower connections panel and status bar. Setup includes
General, NZB and Servers. The executable uses Win32 common controls v6,
the installed Windows visual styles and the existing NewsQueue icons.

## Run

From the repository root in PowerShell:

```powershell
Start-Process .\app\target\native-mockups\debug\newsqueue-native-mockup.exe
```

Choose **View → Roomy** for the taller toolbar and clean tables, or
**View → Compact** for the shorter toolbar and table gridlines. Pass
`--roomy` to start directly in Roomy (`--compact` remains accepted).

Command rows share one native system-color surface, so toolbar, filter label
and spacing have a consistent background. Editable fields retain the native
white surface and border. Setup labels use the same system face/text colors.

Try filtering subjects, expanding the first collection, changing tabs,
toggling the connections panel with F9, pausing the sample download,
opening Setup or using Ctrl+O to add a sample collection. The demo uses
in-memory data and has no connection to the production engine, download
folders, network or settings. Some commands are visual placeholders.
The current Windows version supplies the window frame and native widget
appearance, so the exact appearance depends on the host.

## Build and verify

```powershell
cargo build --locked --manifest-path app/native-mockups/Cargo.toml --target-dir app/target/native-mockups
$exe = Resolve-Path .\app\target\native-mockups\debug\newsqueue-native-mockup.exe
$smoke = Start-Process $exe -ArgumentList '--smoke' -WindowStyle Hidden -PassThru -Wait
if ($smoke.ExitCode -ne 0) { throw "Mockup smoke failed with exit code $($smoke.ExitCode)" }
$capture = Start-Process $exe -ArgumentList '--capture', 'app/target/native-mockups/captures' -WindowStyle Hidden -PassThru -Wait
if ($capture.ExitCode -ne 0) { throw "Mockup capture failed with exit code $($capture.ExitCode)" }
```

The background modes place their windows off-screen without activating them.
The PNGs show Roomy, Compact and Setup. The smoke test checks the native
queue and controls, tab visibility, filtering, collapse and expand, sample
import, pause, connections panel, density selection and Setup lifecycle.
