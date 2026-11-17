# End-to-end check for the native notification-area icon.
# Run from app/ after `cargo build`. Close any running NewsQueue first.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
$queue = Join-Path $root 'newsqueue-queue.json'
$settings = Join-Path $root 'newsqueue-settings.json'
if (-not (Test-Path $exe)) { throw "Build first: $exe is missing" }
if (Get-Process newsqueue -ErrorAction SilentlyContinue) {
    throw 'Close the running NewsQueue instance before running this check.'
}

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

function Test-Offscreen([IntPtr]$handle) {
    $rect = New-Object TrayIconRect
    [void][TrayTest]::GetWindowRect($handle, [ref]$rect)
    return ($rect.Left -le -10000 -and $rect.Top -le -10000)
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
        $main = [TrayTest]::FindWindowByTitle([IntPtr]::Zero, 'NewsQueue 1.0.2')
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
    if ($tooltip -notlike 'NewsQueue 1.0.2*MiB/s*') { throw "unexpected tray tooltip: $tooltip" }
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

    # 1. Minimizing through the titlebar must move the window off-screen,
    # remove its taskbar entry, and keep App::tick running.
    $taskbarBefore = Get-TaskbarButton
    Write-Output "taskbar-before='$taskbarBefore'"
    [void][TrayTest]::ShowWindow($main, 6)
    $minimized = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if (Test-Offscreen $main) { $minimized = $true; break }
    }
    $taskbarAfter = Get-TaskbarButton
    Write-Output "after-minimize visible=$([TrayTest]::IsWindowVisible($main)) offscreen=$minimized taskbar='$taskbarAfter'"
    if (-not $minimized) { throw 'minimize did not move the window off-screen' }
    if ($taskbarBefore -and $taskbarAfter) {
        throw "taskbar entry still present after minimize: $taskbarAfter"
    }

    # App::tick must keep running while off-screen. The 30-second settings
    # autosave is a simple, observable heartbeat.
    $saveBefore = if (Test-Path $settings) { (Get-Item $settings).LastWriteTimeUtc } else { [DateTime]::MinValue }
    $tickRunning = $false
    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 500
        if (Test-Path $settings) {
            $saveAfter = (Get-Item $settings).LastWriteTimeUtc
            if ($saveAfter -gt $saveBefore) { $tickRunning = $true; break }
        }
    }
    Write-Output "offscreen-tick autosaveUpdated=$tickRunning"
    if (-not $tickRunning) { throw 'App::tick did not run while the window was off-screen' }

    # 2. A left click on the icon restores it (WM_TRAY = WM_APP+1, WM_LBUTTONUP).
    [void][TrayTest]::PostMessage($tray, 0x8001, [IntPtr]::Zero, [IntPtr]0x0202)
    $restored = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if ([TrayTest]::IsWindowVisible($main) -and -not [TrayTest]::IsIconic($main) -and -not (Test-Offscreen $main)) { $restored = $true; break }
    }
    Write-Output "after-restore visible=$([TrayTest]::IsWindowVisible($main)) iconic=$([TrayTest]::IsIconic($main)) offscreen=$(Test-Offscreen $main) restored=$restored taskbar='$(Get-TaskbarButton)'"
    if (-not $restored) { throw 'tray click did not restore the window' }

    # 3. Closing hides to the tray instead of exiting.
    [void][TrayTest]::PostMessage($main, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    $closedToTray = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        if ((Test-Offscreen $main) -and -not $proc.HasExited) { $closedToTray = $true; break }
    }
    Write-Output "after-close offscreen=$(Test-Offscreen $main) alive=$(-not $proc.HasExited) closeToTray=$closedToTray"
    if (-not $closedToTray) { throw 'close did not move the window off-screen' }

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
}
