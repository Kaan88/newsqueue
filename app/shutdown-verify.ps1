$ErrorActionPreference = 'Stop'
# The 15-second shutdown countdown (method Exit, armed by the debug-only
# NQ_TEST_ARM_SHUTDOWN hook) must close the app while it is hidden in the
# tray. Runs in a temporary config folder (NQ_CONFIG).
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
if (-not (Test-Path $exe)) { throw 'Build the debug executable first.' }
if (Get-Process newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }) { throw 'Close the debug NewsQueue first.' }
$config = Join-Path $env:TEMP ('newsqueue-shutdown-verify-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $config | Out-Null
$cfg = [ordered]@{
    check_version_minutes = 0; startup_connect = $false
    download_dir = (Join-Path $config 'download'); nzb_dir = (Join-Path $config 'nzb')
    temp_dir = (Join-Path $config 'temp'); unrar_dir = (Join-Path $config 'unrar')
}
[System.IO.File]::WriteAllText((Join-Path $config 'newsqueue-settings.json'), ($cfg | ConvertTo-Json), (New-Object System.Text.UTF8Encoding($false)))
[System.IO.File]::WriteAllText((Join-Path $config 'newsqueue-queue.json'), '[]')

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class ShutdownCheck {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
"@

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $false
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables['NQ_CONFIG'] = $config
$psi.EnvironmentVariables['NQ_TEST_ARM_SHUTDOWN'] = 'exit'
$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $psi
[void]$proc.Start()
try {
    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100 -and $main -eq [IntPtr]::Zero; $i++) {
        Start-Sleep -Milliseconds 200
        $h = [ShutdownCheck]::FindWindow([IntPtr]::Zero, 'NewsQueue 1.0.8')
        $owner = 0
        if ($h -ne [IntPtr]::Zero) { [void][ShutdownCheck]::GetWindowThreadProcessId($h, [ref]$owner) }
        if ($owner -eq $proc.Id) { $main = $h }
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    Start-Sleep -Seconds 1
    # The test hook turns on minimize_to_tray, so minimizing hides the window.
    [void][ShutdownCheck]::ShowWindow($main, 6)
    for ($i = 0; $i -lt 50 -and [ShutdownCheck]::IsWindowVisible($main); $i++) { Start-Sleep -Milliseconds 200 }
    if ([ShutdownCheck]::IsWindowVisible($main)) { throw 'window did not hide to the tray' }
    Write-Output 'hidden-in-tray=True'
    Start-Sleep -Seconds 3
    if ($proc.HasExited) { throw 'the app exited before the shutdown countdown' }
    Write-Output "after-3s hidden=$(-not [ShutdownCheck]::IsWindowVisible($main)) alive=$(-not $proc.HasExited)"
    $exited = $proc.WaitForExit(25000)
    if (-not $exited) { throw 'the app did not exit after the shutdown countdown' }
    Write-Output "shutdown-countdown exitCode=$($proc.ExitCode)"
    if ($proc.ExitCode -ne 0) { throw 'non-zero exit code' }
    Write-Output 'SHUTDOWN-WHILE-HIDDEN PASSED'
} finally {
    if (-not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
    Remove-Item -LiteralPath $config -Recurse -Force -ErrorAction SilentlyContinue
}
