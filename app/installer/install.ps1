param(
    [string]$InstallDir = $env:NQ_INSTALL_DIR,
    [switch]$Silent
)

$ErrorActionPreference = 'Stop'
$version = '1.0.1'
$silentInstall = $Silent -or ($env:NQ_SILENT -eq '1')
$packageDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$shell = New-Object -ComObject WScript.Shell

function Fail([string]$message) {
    if (-not $silentInstall) {
        [void]$shell.Popup("NewsQueue installation failed.`n`n$message", 0, 'NewsQueue Setup', 16)
    }
    Write-Error $message
    exit 1
}

try {
    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\NewsQueue'
    }
    $InstallDir = [System.IO.Path]::GetFullPath($InstallDir)
    if ($InstallDir.Length -gt 3) {
        $InstallDir = $InstallDir.TrimEnd('\')
    }

    $zip = Join-Path $packageDir "NewsQueue-$version.zip"
    $uninstallSource = Join-Path $packageDir 'uninstall.ps1'
    if (-not (Test-Path -LiteralPath $zip)) {
        Fail "Package is missing: $zip"
    }
    if (-not (Test-Path -LiteralPath $uninstallSource)) {
        Fail "Uninstaller is missing: $uninstallSource"
    }

    $running = Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and $_.Path.StartsWith($InstallDir, [System.StringComparison]::OrdinalIgnoreCase)
    }
    if ($null -ne $running) {
        Fail 'NewsQueue is running from this install folder. Close it and run the installer again.'
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $staging = Join-Path $env:TEMP ('newsqueue-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    try {
        Expand-Archive -LiteralPath $zip -DestinationPath $staging -Force
        $inner = Join-Path $staging "NewsQueue-$version"
        if (-not (Test-Path -LiteralPath $inner)) {
            $innerDir = Get-ChildItem -LiteralPath $staging -Directory | Select-Object -First 1
            $inner = if ($null -ne $innerDir) { $innerDir.FullName } else { $null }
        }
        if ($null -ne $inner) {
            Copy-Item -Path (Join-Path $inner '*') -Destination $InstallDir -Recurse -Force
        } else {
            Copy-Item -Path (Join-Path $staging '*') -Destination $InstallDir -Recurse -Force
        }
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
    }

    $uninstallPath = Join-Path $InstallDir 'uninstall.ps1'
    Copy-Item -LiteralPath $uninstallSource -Destination $uninstallPath -Force

    $exe = Join-Path $InstallDir 'newsqueue.exe'
    if (-not (Test-Path -LiteralPath $exe)) {
        Fail 'newsqueue.exe was not installed.'
    }

    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $uninstallArgs = '-NoProfile -ExecutionPolicy Bypass -File "' + $uninstallPath + '"'
    $quietArgs = $uninstallArgs + ' -Silent'

    if ($env:NQ_NO_DESKTOP -ne '1') {
        $desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'NewsQueue.lnk'
        $shortcut = $shell.CreateShortcut($desktopShortcut)
        $shortcut.TargetPath = $exe
        $shortcut.WorkingDirectory = $InstallDir
        $shortcut.IconLocation = "$exe,0"
        $shortcut.Description = 'NewsQueue Usenet downloader'
        $shortcut.Save()
    }

    $startMenuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\NewsQueue'
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
    $shortcut = $shell.CreateShortcut((Join-Path $startMenuDir 'NewsQueue.lnk'))
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.IconLocation = "$exe,0"
    $shortcut.Description = 'NewsQueue Usenet downloader'
    $shortcut.Save()

    $shortcut = $shell.CreateShortcut((Join-Path $startMenuDir 'Uninstall NewsQueue.lnk'))
    $shortcut.TargetPath = $powershell
    $shortcut.Arguments = $uninstallArgs
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.IconLocation = "$exe,0"
    $shortcut.Description = 'Uninstall NewsQueue'
    $shortcut.Save()

    $uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue'
    if (Test-Path -LiteralPath $uninstallKey) {
        Remove-Item -LiteralPath $uninstallKey -Recurse -Force
    }
    New-Item -Path $uninstallKey -Force | Out-Null
    $size = (Get-ChildItem -LiteralPath $InstallDir -Recurse -File | Measure-Object -Property Length -Sum).Sum
    New-ItemProperty -Path $uninstallKey -Name 'DisplayName' -Value "NewsQueue $version" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'DisplayVersion' -Value $version -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'Publisher' -Value 'NewsQueue' -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'InstallLocation' -Value $InstallDir -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'DisplayIcon' -Value "$exe,0" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'UninstallString' -Value ('"' + $powershell + '" ' + $uninstallArgs) -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'QuietUninstallString' -Value ('"' + $powershell + '" ' + $quietArgs) -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'InstallDate' -Value (Get-Date).ToString('yyyyMMdd') -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'EstimatedSize' -Value ([int]($size / 1KB)) -PropertyType DWord -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'NoModify' -Value 1 -PropertyType DWord -Force | Out-Null
    New-ItemProperty -Path $uninstallKey -Name 'NoRepair' -Value 1 -PropertyType DWord -Force | Out-Null

    if (-not $silentInstall) {
        [void]$shell.Popup("NewsQueue $version was installed to`n$InstallDir", 0, 'NewsQueue Setup', 64)
    }
    exit 0
} catch {
    Fail $_.Exception.Message
}
