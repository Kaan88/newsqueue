# End-to-end check for the native notification-area icon.
# Run from app/ after `cargo build`. Uses a temporary config folder
# (NQ_CONFIG), so the settings and queue in app\ are not touched. Minimize
# and close hide the window (SW_HIDE), which also drops its taskbar button.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
if (-not (Test-Path $exe)) { throw "Build first: $exe is missing" }
if (Get-Process newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }) {
    throw 'Close the running debug NewsQueue before running this check.'
}
$config = Join-Path $env:TEMP ('newsqueue-tray-verify-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $config | Out-Null
$queue = Join-Path $config 'newsqueue-queue.json'
$settings = Join-Path $config 'newsqueue-settings.json'
$cfg = [ordered]@{
    minimize_to_tray = $true; minimize_to_tray_on_close = $true; tray_balloon = $true; speed_in_tray = $true
    autosave_minutes = 1; check_version_minutes = 0; startup_connect = $false
    download_dir = (Join-Path $config 'download'); nzb_dir = (Join-Path $config 'nzb')
    temp_dir = (Join-Path $config 'temp'); unrar_dir = (Join-Path $config 'unrar')
}
[System.IO.File]::WriteAllText($settings, ($cfg | ConvertTo-Json), (New-Object System.Text.UTF8Encoding($false)))

Add-Type @"
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential, Pack=8)]
public struct TrayIconId {
  public uint cbSize;
  public IntPtr hWnd;
  public uint uID;
  public Guid guidItem;
}
public struct TrayIconRect { public int Left, Top, Right, Bottom; }
public class TrayTest {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(string cls, string title);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindowByTitle(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("shell32.dll")] public static extern int Shell_NotifyIconGetRect(ref TrayIconId id, out TrayIconRect rect);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out TrayIconRect r);
}
"@
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Get-TaskbarButton {
    try {
        $rootElement = [System.Windows.Automation.AutomationElement]::RootElement
        $condition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Button
        )
        $buttons = $rootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
        for ($i = 0; $i -lt $buttons.Count; $i++) {
            $element = $buttons.Item($i)
            if ($element.Current.Name -like '*newsqueue.exe*' -and
                $element.Current.ClassName -like '*Taskbar*') {
                return $element.Current.Name
            }
        }
    } catch {
        return $null
    }
    return $null
}

function Test-Hidden([IntPtr]$handle) {
    return -not [TrayTest]::IsWindowVisible($handle)
}

function Find-Own([string]$cls, [string]$title) {
    $h = if ($cls) { [TrayTest]::FindWindow($cls, $title) } else { [TrayTest]::FindWindowByTitle([IntPtr]::Zero, $title) }
    if ($h -eq [IntPtr]::Zero) { return [IntPtr]::Zero }
    $owner = 0
    [void][TrayTest]::GetWindowThreadProcessId($h, [ref]$owner)
    if ($owner -ne $proc.Id) { return [IntPtr]::Zero }
    return $h
}

$before = if (Test-Path $queue) { (Get-Item $queue).LastWriteTimeUtc } else { [DateTime]::MinValue }
$tipFile = Join-Path $env:TEMP 'newsqueue-tray-tip.txt'
$balloonFile = Join-Path $env:TEMP 'newsqueue-tray-balloon.txt'
Remove-Item -LiteralPath $tipFile -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $balloonFile -Force -ErrorAction SilentlyContinue
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $false
$psi.RedirectStandardError = $true
$psi.EnvironmentVariables['NQ_CONFIG'] = $config
$psi.EnvironmentVariables['NQ_TRAY_TIP_FILE'] = $tipFile
$psi.EnvironmentVariables['NQ_TRAY_BALLOON_FILE'] = $balloonFile
$psi.EnvironmentVariables['NQ_TRAY_FORCE_BALLOON'] = '1'
$psi.EnvironmentVariables['NQ_TRAY_FORCE_SPEED'] = '1'
$psi.EnvironmentVariables['NQ_TRAY_TEST_SPEED_BPS'] = '13107200'
$proc = New-Object System.Diagnostics.Process
$proc.StartInfo = $psi
[void]$proc.Start()
try {
    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $main = Find-Own $null 'NewsQueue 1.0.7'
        if ($main -ne [IntPtr]::Zero) { break }
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    $tray = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $tray = Find-Own 'NewsQueueTrayWindow' 'NewsQueue Tray'
        if ($tray -ne [IntPtr]::Zero) { break }
    }
    if ($tray -eq [IntPtr]::Zero) { throw 'tray window not found (Shell_NotifyIcon failed)' }
    $iconId = New-Object TrayIconId
    $iconId.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf([type][TrayIconId])
    $iconId.hWnd = $tray
    $iconId.uID = 1
    $iconRect = New-Object TrayIconRect
    $shellHr = [TrayTest]::Shell_NotifyIconGetRect([ref]$iconId, [ref]$iconRect)
    if ($shellHr -ne 0) { throw "tray icon rect lookup failed: HRESULT $shellHr" }
    Write-Output "tray icon rect=$($iconRect.Left),$($iconRect.Top),$($iconRect.Right),$($iconRect.Bottom) shellHr=$shellHr"
    $tooltip = $null
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        if (Test-Path -LiteralPath $tipFile) {
            $tooltip = (Get-Content -LiteralPath $tipFile -Raw).Trim()
            if ($tooltip) { break }
        }
    }
    if (-not $tooltip) { throw 'tray tooltip was not updated' }
    if ($tooltip -notlike 'NewsQueue 1.0.7*MiB/s*') { throw "unexpected tray tooltip: $tooltip" }
    Write-Output "tray tooltip='$tooltip'"

    # The balloon test message asks the tray thread to issue a real
    # Shell_NotifyIcon NIF_INFO request. The hook records the shell result.
    [void][TrayTest]::PostMessage($tray, 0x8003, [IntPtr]::Zero, [IntPtr]::Zero)
    $balloonLines = @()
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        if (Test-Path -LiteralPath $balloonFile) {
            $balloonLines = @(Get-Content -LiteralPath $balloonFile)
            if ($balloonLines.Count -ge 3) { break }
        }
    }
    if ($balloonLines.Count -lt 3) { throw 'tray balloon was not recorded' }
    if ([int]$balloonLines[0] -eq 0) { throw 'Shell_NotifyIcon rejected the balloon request' }
    if ($balloonLines[1] -ne 'NewsQueue' -or $balloonLines[2] -ne 'Tray balloon test') {
        throw "unexpected balloon content: $($balloonLines[1]) / $($balloonLines[2])"
    }
    Write-Output "tray balloon result=$($balloonLines[0]) title='$($balloonLines[1])' text='$($balloonLines[2])'"
    $owner = 0
    [void][TrayTest]::GetWindowThreadProcessId($main, [ref]$owner)
    Write-Output "started pid=$($proc.Id) owner=$owner mainVisible=$([TrayTest]::IsWindowVisible($main)) trayWindow=$tray"

    # 1. Minimizing hides the window (SW_HIDE), which removes its taskbar
    # button; the engine keeps running on its own thread.
    $taskbarBefore = Get-TaskbarButton
    Write-Output "taskbar-before='$taskbarBefore'"
    [void][TrayTest]::ShowWindow($main, 6)
    $minimized = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if (Test-Hidden $main) { $minimized = $true; break }
    }
    $taskbarAfter = Get-TaskbarButton
    Write-Output "after-minimize hidden=$minimized taskbar='$taskbarAfter'"
    if (-not $minimized) { throw 'minimize did not hide the window to the tray' }
    if ($taskbarBefore -and $taskbarAfter) {
        throw "taskbar entry still present after minimize: $taskbarAfter"
    }

    # The engine must keep ticking while hidden. The queue autosave (every
    # minute here) is a simple, observable heartbeat.
    $saveBefore = if (Test-Path $queue) { (Get-Item $queue).LastWriteTimeUtc } else { [DateTime]::MinValue }
    $tickRunning = $false
    for ($i = 0; $i -lt 160; $i++) {
        Start-Sleep -Milliseconds 500
        if (Test-Path $queue) {
            $saveAfter = (Get-Item $queue).LastWriteTimeUtc
            if ($saveAfter -gt $saveBefore) { $tickRunning = $true; break }
        }
    }
    Write-Output "hidden-tick autosaveUpdated=$tickRunning"
    if (-not $tickRunning) { throw 'the engine did not tick while the window was hidden' }

    # 2. A left click on the icon restores it (WM_TRAY = WM_APP+1, WM_LBUTTONUP).
    [void][TrayTest]::PostMessage($tray, 0x8001, [IntPtr]::Zero, [IntPtr]0x0202)
    $restored = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if ([TrayTest]::IsWindowVisible($main) -and -not [TrayTest]::IsIconic($main)) { $restored = $true; break }
    }
    Write-Output "after-restore visible=$([TrayTest]::IsWindowVisible($main)) iconic=$([TrayTest]::IsIconic($main)) restored=$restored taskbar='$(Get-TaskbarButton)'"
    if (-not $restored) { throw 'tray click did not restore the window' }

    # 3. Closing hides to the tray instead of exiting.
    [void][TrayTest]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    $closedToTray = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if ((Test-Hidden $main) -and -not $proc.HasExited) { $closedToTray = $true; break }
    }
    Write-Output "after-close hidden=$(Test-Hidden $main) alive=$(-not $proc.HasExited) closeToTray=$closedToTray"
    if (-not $closedToTray) { throw 'close did not hide the window to the tray' }

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
    Remove-Item -LiteralPath $tipFile -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $balloonFile -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $config -Recurse -Force -ErrorAction SilentlyContinue
}
