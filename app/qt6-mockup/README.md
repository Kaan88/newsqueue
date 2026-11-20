# NewsQueue Qt 6 Compact mockup

Open [run.cmd](run.cmd) to launch the local preview. The executable and its Qt
runtime are under `app/target/qt6-mockup/preview`. The preview defaults to
Compact and uses the Windows window frame.

This standalone Qt Widgets prototype follows the approved native Win32
Compact reference. It uses the same fonts, glyphs, column widths, table
density, sample queue and connection data. The toolbar, menu, tabs, queue,
status bar and Setup use Qt widgets. The small transparent PNGs in `assets/`
were exported from the existing NewsQueue icon drawing code in
`app/src/util.rs`.

Try filtering subjects, expanding the first collection with its icon or
Left/Right, pausing, using Ctrl+O for a sample import, toggling connections
with F9, switching tabs and opening Setup. The demo holds state in memory.
It uses no download engine, network connections or production settings.
Unavailable commands are disabled. To tray minimizes the preview, and sample
import adds an in-memory fixture.

## Build

From the repository root in PowerShell, with Qt 6 for MSVC x64, CMake, Visual
Studio C++ build tools and the Visual C++ runtime installed:

```powershell
powershell -ExecutionPolicy Bypass -File .\app\qt6-mockup\build.ps1
```

Use `-QtRoot` to select a different installed Qt MSVC kit. The default on this
machine is `C:\Qt\6.9.3\msvc2022_64`.

## Verify offscreen

```powershell
$exe = Resolve-Path .\app\target\qt6-mockup\preview\newsqueue-qt-compact.exe
$smoke = Start-Process $exe -ArgumentList '--smoke' -WindowStyle Hidden -Wait -PassThru
if ($smoke.ExitCode -ne 0) { throw "Qt mockup smoke failed ($($smoke.ExitCode))." }
$capture = Start-Process $exe -ArgumentList '--capture', 'app/target/qt6-mockup/captures' -WindowStyle Hidden -Wait -PassThru
if ($capture.ExitCode -ne 0) { throw "Qt mockup capture failed ($($capture.ExitCode))." }
```

Both arguments select Qt's offscreen backend before startup. The smoke check
covers the active and paused queue, filter, expand and collapse, connection
toggle, tabs, panel visibility, density, sample import, cleanup and Setup
pages. Captures cover the 1269×791 Compact client area and the 720×576 Setup
client area at 96 DPI. Qt and Win32 render text, control chrome and the
scrollbar differently. The offscreen PNGs omit the Windows caption and border.
