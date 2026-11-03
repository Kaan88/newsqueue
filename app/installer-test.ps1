$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.0'
$setup = Join-Path $root "dist\NewsQueue-$version-Setup.exe"
$testDir = Join-Path $env:TEMP 'newsqueue-install-test'
$startMenuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\NewsQueue'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue'
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'NewsQueue.lnk'
$screenshot = Join-Path $env:TEMP 'newsqueue-installed-shot.png'

if (-not (Test-Path -LiteralPath $setup)) {
    throw "Build the installer first: $setup is missing"
}
if (Get-Process -Name newsqueue -ErrorAction SilentlyContinue) {
    throw 'Close the running NewsQueue instance before testing the installer.'
}
if ((Test-Path -LiteralPath $uninstallKey) -or (Test-Path -LiteralPath $startMenuDir) -or (Test-Path -LiteralPath $desktopShortcut)) {
    throw 'A NewsQueue installation or shortcut already exists. Remove it before running this test.'
}

$oldInstallDir = $env:NQ_INSTALL_DIR
$oldSilent = $env:NQ_SILENT
$oldNoDesktop = $env:NQ_NO_DESKTOP
$installed = $false

try {
    Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $screenshot -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $uninstallKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $startMenuDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue

    $env:NQ_INSTALL_DIR = $testDir
    $env:NQ_SILENT = '1'
    $env:NQ_NO_DESKTOP = ''
    $process = Start-Process -FilePath $setup -Wait -PassThru
    Write-Output "setup exit=$($process.ExitCode)"

    for ($i = 0; $i -lt 120; $i++) {
        if (Test-Path -LiteralPath (Join-Path $testDir 'newsqueue.exe')) {
            break
        }
        Start-Sleep -Milliseconds 250
    }
    $installedExe = Join-Path $testDir 'newsqueue.exe'
    if (-not (Test-Path -LiteralPath $installedExe)) {
        throw 'The installer did not create newsqueue.exe.'
    }
    $installed = $true

    $required = @(
        'newsqueue.exe',
        'newsqueue-256.png',
        'README.md',
        'ROADMAP.md',
        'tools\par2.exe',
        'tools\UnRAR.exe',
        'tools\LICENSE-par2.txt',
        'tools\LICENSE-UnRAR.txt',
        'uninstall.ps1'
    )
    foreach ($relative in $required) {
        $full = Join-Path $testDir $relative
        if (-not (Test-Path -LiteralPath $full)) {
            throw "Installed file is missing: $relative"
        }
        Write-Output ("installed {0} {1} bytes" -f $relative, (Get-Item -LiteralPath $full).Length)
    }

    $properties = Get-ItemProperty -LiteralPath $uninstallKey
    if ($properties.DisplayName -ne "NewsQueue $version") {
        throw "Unexpected DisplayName: $($properties.DisplayName)"
    }
    if ($properties.InstallLocation -ne $testDir) {
        throw "Unexpected InstallLocation: $($properties.InstallLocation)"
    }
    if ($properties.UninstallString -notlike '*uninstall.ps1*') {
        throw 'UninstallString does not point to uninstall.ps1'
    }
    if ($properties.QuietUninstallString -notlike '*-Silent*') {
        throw 'QuietUninstallString is missing -Silent'
    }
    if ($properties.EstimatedSize -le 0) {
        throw 'EstimatedSize was not written'
    }
    Write-Output "registry DisplayName=$($properties.DisplayName) size=$($properties.EstimatedSize) KB"

    $menuShortcut = Join-Path $startMenuDir 'NewsQueue.lnk'
    $menuUninstall = Join-Path $startMenuDir 'Uninstall NewsQueue.lnk'
    if (-not (Test-Path -LiteralPath $menuShortcut)) {
        throw 'Start Menu shortcut was not created'
    }
    if (-not (Test-Path -LiteralPath $menuUninstall)) {
        throw 'Start Menu uninstall shortcut was not created'
    }
    if (-not (Test-Path -LiteralPath $desktopShortcut)) {
        throw 'Desktop shortcut was not created'
    }
    Write-Output 'start menu and desktop shortcuts present'

    $installedRun = Start-Process -FilePath $installedExe -ArgumentList @('--screenshot', $screenshot) -WorkingDirectory $testDir -Wait -PassThru
    if ($installedRun.ExitCode -ne 0) {
        throw "The installed executable exited with $($installedRun.ExitCode)"
    }
    if (-not (Test-Path -LiteralPath $screenshot)) {
        throw 'The installed executable could not take a screenshot'
    }
    Write-Output "installed executable ran; screenshot=$((Get-Item -LiteralPath $screenshot).Length) bytes"

    $uninstall = Join-Path $testDir 'uninstall.ps1'
    & (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -NoProfile -ExecutionPolicy Bypass -File $uninstall -Silent
    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        if (-not (Test-Path -LiteralPath $testDir)) {
            break
        }
    }
    if (Test-Path -LiteralPath $testDir) {
        throw "Uninstaller did not remove $testDir"
    }
    if (Test-Path -LiteralPath $uninstallKey) {
        throw 'Uninstaller left the registry key behind'
    }
    if (Test-Path -LiteralPath $startMenuDir) {
        throw 'Uninstaller left the Start Menu folder behind'
    }
    if (Test-Path -LiteralPath $desktopShortcut) {
        throw 'Uninstaller left the Desktop shortcut behind'
    }
    Write-Output 'uninstall removed files, shortcuts and registry entry'
    Write-Output 'INSTALLER VERIFICATION PASSED'
} finally {
    $env:NQ_INSTALL_DIR = $oldInstallDir
    $env:NQ_SILENT = $oldSilent
    $env:NQ_NO_DESKTOP = $oldNoDesktop
    if (Get-Process -Name newsqueue -ErrorAction SilentlyContinue) {
        Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Stop-Process -Force
    }
    if (Test-Path -LiteralPath $testDir) {
        Remove-Item -LiteralPath $testDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $uninstallKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $startMenuDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $desktopShortcut -Force -ErrorAction SilentlyContinue
}
