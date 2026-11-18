# End-to-end check of the Inno Setup installer: an update over an IExpress
# install started the old way (environment variables only), a re-install
# with Inno flags, the installed exe, and the uninstaller.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.5'
$setup = Join-Path $root "dist\NewsQueue-$version-Setup.exe"
$testDir = Join-Path $env:TEMP 'newsqueue-install-test'
$startMenuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\NewsQueue'
$oldKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue'
$innoKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{7A3E2C51-9B4D-4F0E-8C21-6D5B9E1A4F37}_is1'
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'NewsQueue.lnk'
$screenshot = Join-Path $env:TEMP 'newsqueue-installed-shot.png'

function Check([bool]$ok, [string]$message) {
    if (-not $ok) { throw "FAILED: $message" }
    Write-Output "ok: $message"
}

# Setup.exe hands over to a temporary is-*.tmp process; wait for both.
function Wait-Setup {
    for ($i = 0; $i -lt 240; $i++) {
        Start-Sleep -Milliseconds 250
        $busy = Get-Process | Where-Object { $_.ProcessName -like 'NewsQueue-*-Setup*' -or $_.ProcessName -like '*.tmp' -or $_.ProcessName -like 'is-*' -or $_.ProcessName -like 'unins*' }
        if (-not $busy -and $i -gt 4) { return }
    }
    throw 'Setup did not finish within a minute'
}

if (-not (Test-Path -LiteralPath $setup)) {
    throw "Build the installer first: $setup is missing"
}
if (Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$testDir*" }) {
    throw "Close the NewsQueue running from $testDir before testing the installer."
}
if ((Test-Path -LiteralPath $oldKey) -or (Test-Path -LiteralPath $innoKey) -or (Test-Path -LiteralPath $startMenuDir) -or (Test-Path -LiteralPath $desktopShortcut)) {
    throw 'A NewsQueue installation or shortcut already exists. Remove it before running this test.'
}

$saved = @{ NQ_INSTALL_DIR = $env:NQ_INSTALL_DIR; NQ_SILENT = $env:NQ_SILENT; NQ_NO_DESKTOP = $env:NQ_NO_DESKTOP }
try {
    Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $screenshot -Force -ErrorAction SilentlyContinue

    # 1. Pretend an IExpress install (1.0.5) is there, with user settings.
    New-Item -ItemType Directory -Force -Path $testDir, $startMenuDir | Out-Null
    Set-Content -LiteralPath (Join-Path $testDir 'uninstall.ps1') -Value '# old uninstaller'
    Set-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Value '{"marker":"keep-me"}'
    Set-Content -LiteralPath (Join-Path $startMenuDir 'Uninstall NewsQueue.lnk') -Value 'old'
    New-Item -Path $oldKey -Force | Out-Null
    New-ItemProperty -Path $oldKey -Name 'DisplayName' -Value 'NewsQueue 1.0.5' -PropertyType String -Force | Out-Null

    # 2. Update it the way 1.0.2-1.0.5 copies start a setup: env vars only.
    $env:NQ_INSTALL_DIR = $testDir
    $env:NQ_SILENT = '1'
    $env:NQ_NO_DESKTOP = '1'
    $p = Start-Process -FilePath $setup -Wait -PassThru
    Write-Output "env-var setup handed over (exit=$($p.ExitCode))"
    Wait-Setup
    $env:NQ_SILENT = $saved.NQ_SILENT; $env:NQ_INSTALL_DIR = $saved.NQ_INSTALL_DIR; $env:NQ_NO_DESKTOP = $saved.NQ_NO_DESKTOP

    $required = @('newsqueue.exe', 'newsqueue-256.png', 'README.md', 'ROADMAP.md', 'tools\par2.exe', 'tools\UnRAR.exe',
        'tools\LICENSE-par2.txt', 'tools\LICENSE-UnRAR.txt', 'unins000.exe')
    foreach ($relative in $required) {
        Check (Test-Path -LiteralPath (Join-Path $testDir $relative)) "installed $relative"
    }
    Check (-not (Test-Path -LiteralPath $oldKey)) 'old IExpress uninstall entry removed'
    Check (-not (Test-Path -LiteralPath (Join-Path $testDir 'uninstall.ps1'))) 'old uninstall.ps1 removed'
    Check ((Get-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Raw) -match 'keep-me') 'settings kept'
    $props = Get-ItemProperty -LiteralPath $innoKey
    Check ($props.DisplayName -eq "NewsQueue $version") "uninstall entry DisplayName=$($props.DisplayName)"
    Check ($props.InstallLocation.TrimEnd('\') -eq $testDir) "InstallLocation=$($props.InstallLocation)"
    Check ($props.UninstallString -like '*unins000.exe*') 'UninstallString points to unins000.exe'
    Check (Test-Path -LiteralPath (Join-Path $startMenuDir 'NewsQueue.lnk')) 'Start Menu shortcut'
    $uninstallLnk = Join-Path $startMenuDir 'Uninstall NewsQueue.lnk'
    Check ((Test-Path -LiteralPath $uninstallLnk) -and ((Get-Item -LiteralPath $uninstallLnk).Length -gt 10)) 'Start Menu uninstall shortcut is the new one'
    Check (-not (Test-Path -LiteralPath $desktopShortcut)) 'no Desktop shortcut with NQ_NO_DESKTOP=1'

    # 3. Re-install with Inno flags, as 1.0.6+ self-updates do, plus the desktop task.
    $p = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=$testDir", '/MERGETASKS=desktopicon') -Wait -PassThru
    Wait-Setup
    Check ($p.ExitCode -eq 0) "flag setup exit=$($p.ExitCode)"
    Check (Test-Path -LiteralPath $desktopShortcut) 'Desktop shortcut with the desktopicon task'
    Check ((Get-Content -LiteralPath (Join-Path $testDir 'newsqueue-settings.json') -Raw) -match 'keep-me') 'settings kept on re-install'

    # 4. The installed exe runs.
    $run = Start-Process -FilePath (Join-Path $testDir 'newsqueue.exe') -ArgumentList @('--screenshot', $screenshot) -WorkingDirectory $testDir -Wait -PassThru
    Check (($run.ExitCode -eq 0) -and (Test-Path -LiteralPath $screenshot)) "installed exe took a screenshot ($((Get-Item -LiteralPath $screenshot).Length) bytes)"

    # 5. Uninstall.
    $p = Start-Process -FilePath (Join-Path $testDir 'unins000.exe') -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -Wait -PassThru
    Wait-Setup
    Check (-not (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue.exe'))) 'program files removed'
    Check (-not (Test-Path -LiteralPath $innoKey)) 'uninstall entry removed'
    Check (-not (Test-Path -LiteralPath $startMenuDir)) 'Start Menu folder removed'
    Check (-not (Test-Path -LiteralPath $desktopShortcut)) 'Desktop shortcut removed'
    Check (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue-settings.json')) 'user settings left in place'
    Write-Output 'INSTALLER VERIFICATION PASSED'
} finally {
    $env:NQ_INSTALL_DIR = $saved.NQ_INSTALL_DIR
    $env:NQ_SILENT = $saved.NQ_SILENT
    $env:NQ_NO_DESKTOP = $saved.NQ_NO_DESKTOP
    Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$testDir*" } | Stop-Process -Force
    Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $oldKey, $innoKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $startMenuDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue
}
