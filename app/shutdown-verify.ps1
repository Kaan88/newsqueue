$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
$settings = Join-Path $root 'newsqueue-settings.json'
$queue = Join-Path $root 'newsqueue-queue.json'
$history = Join-Path $root 'newsqueue-history.json'
$stats = Join-Path $root 'newsqueue-stats.json'
if (-not (Test-Path $exe)) { throw 'Build the debug executable first.' }
if (Get-Process newsqueue -ErrorAction SilentlyContinue) { throw 'Close NewsQueue first.' }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public struct RECTS { public int Left, Top, Right, Bottom; }
public class ShutdownCheck {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECTS r);
}
"@

$backup = Join-Path $env:TEMP ('newsqueue-shutdown-backup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $backup | Out-Null
foreach ($file in @($settings, $queue, $history, $stats)) {
    if (Test-Path -LiteralPath $file) { Copy-Item -LiteralPath $file -Destination $backup -Force }
}
Set-Content -LiteralPath $queue -Value '[]' -Encoding utf8

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $false
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables['NQ_TEST_ARM_SHUTDOWN'] = 'exit'
$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $psi
[void]$proc.Start()
try {
    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $main = [ShutdownCheck]::FindWindow([IntPtr]::Zero, 'NewsQueue 1.0.4')
        if ($main -ne [IntPtr]::Zero) { break }
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    Start-Sleep -Seconds 1
    [void][ShutdownCheck]::ShowWindow($main, 6)
    $rect = New-Object RECTS
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        [void][ShutdownCheck]::GetWindowRect($main, [ref]$rect)
        if ($rect.Left -le -10000) { break }
    }
    if ($rect.Left -gt -10000) { throw 'window did not move off-screen' }
    Write-Output "offscreen rect=$($rect.Left),$($rect.Top),$($rect.Right),$($rect.Bottom)"
    Start-Sleep -Seconds 3
    if ($proc.HasExited) { throw 'the app exited before the shutdown countdown' }
    Write-Output "after-3s offscreen=$($rect.Left -le -10000) alive=$(-not $proc.HasExited)"
    $exited = $proc.WaitForExit(25000)
    if (-not $exited) { throw 'the app did not exit after the shutdown countdown' }
    Write-Output "shutdown-countdown exitCode=$($proc.ExitCode)"
    if ($proc.ExitCode -ne 0) { throw 'non-zero exit code' }
    Write-Output 'SHUTDOWN-WHILE-OFFSCREEN PASSED'
} finally {
    if (-not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
    foreach ($file in @($settings, $queue, $history, $stats)) {
        $name = Split-Path -Leaf $file
        $saved = Join-Path $backup $name
        if (Test-Path -LiteralPath $saved) {
            Copy-Item -LiteralPath $saved -Destination $file -Force
        } elseif (Test-Path -LiteralPath $file) {
            Remove-Item -LiteralPath $file -Force
        }
    }
    Remove-Item -LiteralPath $backup -Recurse -Force -ErrorAction SilentlyContinue
}
