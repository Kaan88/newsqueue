# End-to-end check for the native notification-area icon.
# Run from app/ after `cargo build`. Close any running NewsQueue first.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
$queue = Join-Path $root 'newsqueue-queue.json'
if (-not (Test-Path $exe)) { throw "Build first: $exe is missing" }
if (Get-Process newsqueue -ErrorAction SilentlyContinue) {
    throw 'Close the running NewsQueue instance before running this check.'
}

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class TrayTest {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(string cls, string title);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindowByTitle(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
"@

$before = if (Test-Path $queue) { (Get-Item $queue).LastWriteTimeUtc } else { [DateTime]::MinValue }
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $false
$psi.RedirectStandardError = $true
$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $psi
[void]$proc.Start()
try {
    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $main = [TrayTest]::FindWindowByTitle([IntPtr]::Zero, 'NewsQueue 0.48.4.0')
        if ($main -ne [IntPtr]::Zero) { break }
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    $tray = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $tray = [TrayTest]::FindWindow('NewsQueueTrayWindow', 'NewsQueue Tray')
        if ($tray -ne [IntPtr]::Zero) { break }
    }
    if ($tray -eq [IntPtr]::Zero) { throw 'tray window not found (Shell_NotifyIcon failed)' }
    $owner = 0
    [void][TrayTest]::GetWindowThreadProcessId($main, [ref]$owner)
    Write-Output "started pid=$($proc.Id) owner=$owner mainVisible=$([TrayTest]::IsWindowVisible($main)) trayWindow=$tray"

    # 1. Minimizing through the titlebar must hide the window to the tray.
    [void][TrayTest]::ShowWindow($main, 6)
    $minimized = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if (-not [TrayTest]::IsWindowVisible($main)) { $minimized = $true; break }
    }
    Write-Output "after-minimize visible=$([TrayTest]::IsWindowVisible($main)) hiddenToTray=$minimized"
    if (-not $minimized) { throw 'minimize did not hide the window' }

    # 2. A left click on the icon restores it (WM_TRAY = WM_APP+1, WM_LBUTTONUP).
    [void][TrayTest]::PostMessage($tray, 0x8001, [IntPtr]::Zero, [IntPtr]0x0202)
    $restored = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if ([TrayTest]::IsWindowVisible($main) -and -not [TrayTest]::IsIconic($main)) { $restored = $true; break }
    }
    Write-Output "after-restore visible=$([TrayTest]::IsWindowVisible($main)) iconic=$([TrayTest]::IsIconic($main)) restored=$restored"
    if (-not $restored) { throw 'tray click did not restore the window' }

    # 3. Closing hides to the tray instead of exiting.
    [void][TrayTest]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    $closedToTray = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if (-not [TrayTest]::IsWindowVisible($main) -and -not $proc.HasExited) { $closedToTray = $true; break }
    }
    Write-Output "after-close hidden=$(-not [TrayTest]::IsWindowVisible($main)) alive=$(-not $proc.HasExited) closeToTray=$closedToTray"
    if (-not $closedToTray) { throw 'close did not hide to the tray' }

    # 4. The tray Exit command exits cleanly (WM_TRAY_EXIT = WM_APP+2).
    [void][TrayTest]::PostMessage($tray, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero)
    $exited = $proc.WaitForExit(10000)
    if (-not $exited) { throw 'tray exit did not stop the process' }
    $code = $proc.ExitCode
    $after = if (Test-Path $queue) { (Get-Item $queue).LastWriteTimeUtc } else { [DateTime]::MinValue }
    Write-Output "after-tray-exit exited=$exited exitCode=$code queueSaved=$($after -gt $before)"
    if ($code -ne 0) { throw "non-zero exit code: $code" }
    $stderr = $proc.StandardError.ReadToEnd()
    if ($stderr) { Write-Output $stderr.Trim() }
    Write-Output 'TRAY VERIFICATION PASSED'
} finally {
    if (-not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
}
