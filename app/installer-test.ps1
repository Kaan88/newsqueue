# End-to-end check of the Inno Setup installer: an update over an IExpress
# install started the old way (environment variables only), a re-install
# with Inno flags, the installed exe, and the uninstaller.
#
# The test builds its own variant of installer\newsqueue.iss from the
# portable folder (package.ps1) with a different name and AppId, so its
# shortcuts, uninstall entry and the IExpress clean-up never touch a real
# NewsQueue install on this machine; everything else is the real script.
param([switch]$Hidden)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.8'
$name = 'NewsQueue Install Test'
$guid = 'C0FFEE00-5E7A-4B1D-9A2C-7E57DEADBEEF'
$folder = Join-Path $root "dist\NewsQueue-$version"
$work = Join-Path $env:TEMP 'newsqueue-install-test-build'
$setup = Join-Path $work 'NewsQueueInstallTest-Setup.exe'
$testDir = Join-Path $env:TEMP 'newsqueue-install-test'
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$name"
$oldKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$name"
$innoKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{$guid}_is1"
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) "$name.lnk"
$screenshot = Join-Path $env:TEMP 'newsqueue-installed-shot.png'

# Resolve and validate every recursive filesystem cleanup target up front.
$tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
foreach ($path in @($work, $testDir)) {
    if (-not [IO.Path]::GetFullPath($path).StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe test path: $path" }
}
$menuRoot = [IO.Path]::GetFullPath((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')).TrimEnd('\') + '\'
if (-not [IO.Path]::GetFullPath($startMenuDir).StartsWith($menuRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe shortcut path' }

function Check([bool]$ok, [string]$message) {
    if (-not $ok) { throw "FAILED: $message" }
    Write-Output "ok: $message"
}

# Setup.exe hands over to a temporary is-*.tmp process; wait for both.
function Wait-Setup {
    for ($i = 0; $i -lt 240; $i++) {
        Start-Sleep -Milliseconds 250
        $busy = Get-Process | Where-Object { $_.ProcessName -like 'NewsQueue*Setup*' -or $_.ProcessName -like '*.tmp' -or $_.ProcessName -like 'is-*' -or $_.ProcessName -like 'unins*' }
        if (-not $busy -and $i -gt 4) { return }
    }
    throw 'Setup did not finish within a minute'
}

if (-not (Test-Path -LiteralPath (Join-Path $folder 'newsqueue.exe'))) {
    throw "Build the portable folder first (package.ps1): $folder is missing"
}
$iscc = @($env:NQ_ISCC, 'C:\Program Files\Inno Setup 7\ISCC.exe', 'C:\Program Files (x86)\Inno Setup 7\ISCC.exe') |
    Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
if (-not $iscc) { throw 'ISCC.exe not found: install Inno Setup 7 or set NQ_ISCC.' }
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
& $iscc /Q "/DAppVersion=$version" "/DDistDir=$folder" "/DOutputDir=$work" "/DAppName=$name" "/DAppGuid=$guid" '/DOutputName=NewsQueueInstallTest-Setup' (Join-Path $root 'installer\newsqueue.iss')
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $setup)) { throw "ISCC failed with exit code $LASTEXITCODE" }
if (Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$testDir*" }) {
    throw "Close the NewsQueue running from $testDir before testing the installer."
}
if ((Test-Path -LiteralPath $oldKey) -or (Test-Path -LiteralPath $innoKey) -or (Test-Path -LiteralPath $startMenuDir) -or (Test-Path -LiteralPath $desktopShortcut)) {
    throw "A '$name' entry or shortcut is left from an earlier run. Remove it before running this test."
}

$saved = @{ NQ_INSTALL_DIR = $env:NQ_INSTALL_DIR; NQ_SILENT = $env:NQ_SILENT; NQ_NO_DESKTOP = $env:NQ_NO_DESKTOP; NQ_WAIT_PID = $env:NQ_WAIT_PID; NQ_RELAUNCH = $env:NQ_RELAUNCH }
try {
    Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $screenshot -Force -ErrorAction SilentlyContinue

    # 1. Pretend an IExpress install (1.0.5) is there, with user settings.
    New-Item -ItemType Directory -Force -Path $testDir, $startMenuDir | Out-Null
    Set-Content -LiteralPath (Join-Path $testDir 'uninstall.ps1') -Value '# old uninstaller'
    Set-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Value '{"marker":"keep-me"}'
    Set-Content -LiteralPath (Join-Path $testDir 'newsqueue-picker-folders.json') -Value '{"marker":"keep-folder"}'
    Set-Content -LiteralPath (Join-Path $startMenuDir "Uninstall $name.lnk") -Value 'old'
    New-Item -Path $oldKey -Force | Out-Null
    New-ItemProperty -Path $oldKey -Name 'DisplayName' -Value "$name $version" -PropertyType String -Force | Out-Null

    # 2. Update it the way 1.0.2-1.0.5 copies start a setup: env vars only.
    $env:NQ_INSTALL_DIR = $testDir
    $env:NQ_SILENT = '1'
    $env:NQ_NO_DESKTOP = '1'
    # Like update.rs in 1.0.2-1.0.5: wait for the old process, then relaunch.
    $old = Start-Process -FilePath powershell -ArgumentList '-NoProfile', '-Command', 'Start-Sleep 4' -WindowStyle Hidden -PassThru
    $env:NQ_WAIT_PID = "$($old.Id)"
    $env:NQ_RELAUNCH = '1'
    if ($Hidden) { $env:NQ_RELAUNCH = '' }
    $started = Get-Date
    # Not -Wait: in PowerShell 5.1 it also waits for the relaunched app.
    if ($Hidden) {
        $p = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=$testDir", '/MERGETASKS=!desktopicon') -WindowStyle Hidden -PassThru
    } else {
        $p = Start-Process -FilePath $setup -WindowStyle Hidden -PassThru
    }
    $null = $p.Handle
    $p.WaitForExit()
    Wait-Setup
    Check ($old.HasExited) 'setup waited for the old process (NQ_WAIT_PID)'
    Check (((Get-Date) - $started).TotalSeconds -ge 3) 'install finished after the old process exited'
    if (-not $Hidden) {
    $relaunched = $null
    for ($i = 0; $i -lt 40 -and -not $relaunched; $i++) {
        Start-Sleep -Milliseconds 250
        $relaunched = Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$testDir*" }
    }
    Check ($null -ne $relaunched) 'installed NewsQueue relaunched (NQ_RELAUNCH)'
    $relaunched | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    } else { Write-Output 'Hidden mode: legacy wizard and visible relaunch checks skipped; installed executable checked off-screen below.' }
    $env:NQ_SILENT = $saved.NQ_SILENT; $env:NQ_INSTALL_DIR = $saved.NQ_INSTALL_DIR; $env:NQ_NO_DESKTOP = $saved.NQ_NO_DESKTOP
    $env:NQ_WAIT_PID = ''; $env:NQ_RELAUNCH = ''

    $required = @('newsqueue.exe', 'newsqueue-256.png', 'README.md', 'ROADMAP.md', 'tools\par2.exe', 'tools\UnRAR.exe',
        'tools\LICENSE-par2.txt', 'tools\LICENSE-UnRAR.txt', 'unins000.exe')
    foreach ($relative in $required) {
        Check (Test-Path -LiteralPath (Join-Path $testDir $relative)) "installed $relative"
    }
    Check (-not (Test-Path -LiteralPath $oldKey)) 'old IExpress uninstall entry removed'
    Check (-not (Test-Path -LiteralPath (Join-Path $testDir 'uninstall.ps1'))) 'old uninstall.ps1 removed'
    Check ((Get-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Raw) -match 'keep-me') 'settings kept'
    $props = Get-ItemProperty -LiteralPath $innoKey
    Check ($props.DisplayName -eq "$name $version") "uninstall entry DisplayName=$($props.DisplayName)"
    Check ($props.InstallLocation.TrimEnd('\') -eq $testDir) "InstallLocation=$($props.InstallLocation)"
    Check ($props.UninstallString -like '*unins000.exe*') 'UninstallString points to unins000.exe'
    Check (Test-Path -LiteralPath (Join-Path $startMenuDir "$name.lnk")) 'Start Menu shortcut'
    $uninstallLnk = Join-Path $startMenuDir "Uninstall $name.lnk"
    Check ((Test-Path -LiteralPath $uninstallLnk) -and ((Get-Item -LiteralPath $uninstallLnk).Length -gt 10)) 'Start Menu uninstall shortcut is the new one'
    Check (-not (Test-Path -LiteralPath $desktopShortcut)) 'no Desktop shortcut with NQ_NO_DESKTOP=1'

    # 3. Re-install with Inno flags, as 1.0.6+ self-updates do, plus the desktop task.
    $p = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=$testDir", '/MERGETASKS=desktopicon') -WindowStyle Hidden -Wait -PassThru
    Wait-Setup
    Check ($p.ExitCode -eq 0) "flag setup exit=$($p.ExitCode)"
    Check (Test-Path -LiteralPath $desktopShortcut) 'Desktop shortcut with the desktopicon task'
    Check ((Get-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Raw) -match 'keep-me') 'settings kept on re-install'
    Check ((Get-Content -LiteralPath (Join-Path $testDir 'newsqueue-picker-folders.json') -Raw) -match 'keep-folder') 'folder memory kept on re-install'

    # 4. The installed exe runs.
    $run = Start-Process -FilePath (Join-Path $testDir 'newsqueue.exe') -ArgumentList @('--screenshot', $screenshot) -WorkingDirectory $testDir -WindowStyle Hidden -Wait -PassThru
    Check (($run.ExitCode -eq 0) -and (Test-Path -LiteralPath $screenshot)) "installed exe took a screenshot ($((Get-Item -LiteralPath $screenshot).Length) bytes)"

    # 5. Uninstall.
    $p = Start-Process -FilePath (Join-Path $testDir 'unins000.exe') -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -WindowStyle Hidden -Wait -PassThru
    Wait-Setup
    Check (-not (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue.exe'))) 'program files removed'
    Check (-not (Test-Path -LiteralPath $innoKey)) 'uninstall entry removed'
    Check (-not (Test-Path -LiteralPath $startMenuDir)) 'Start Menu folder removed'
    Check (-not (Test-Path -LiteralPath $desktopShortcut)) 'Desktop shortcut removed'
    Check (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue-settings.json')) 'user settings left in place'
    Check (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue-picker-folders.json')) 'folder memory left in place'
    Write-Output 'INSTALLER VERIFICATION PASSED'
} finally {
    $env:NQ_INSTALL_DIR = $saved.NQ_INSTALL_DIR
    $env:NQ_SILENT = $saved.NQ_SILENT
    $env:NQ_NO_DESKTOP = $saved.NQ_NO_DESKTOP
    $env:NQ_WAIT_PID = $saved.NQ_WAIT_PID
    $env:NQ_RELAUNCH = $saved.NQ_RELAUNCH
    Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$testDir*" } | Stop-Process -Force
    Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $oldKey, $innoKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $startMenuDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
