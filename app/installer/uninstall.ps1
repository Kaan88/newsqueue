param(
    [switch]$Silent
)

$ErrorActionPreference = 'Stop'
$version = '1.0.3'
$silentUninstall = $Silent -or ($env:NQ_SILENT -eq '1')
$installDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ($installDir.Length -gt 3) {
    $installDir = $installDir.TrimEnd('\')
}
$shell = New-Object -ComObject WScript.Shell

if (-not $silentUninstall) {
    $answer = $shell.Popup("Remove NewsQueue $version and its files?", 0, 'Uninstall NewsQueue', 4 + 32)
    if ($answer -ne 6) {
        exit 0
    }
}

Get-Process -Name newsqueue -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and $_.Path.StartsWith($installDir, [System.StringComparison]::OrdinalIgnoreCase)
} | Stop-Process -Force -ErrorAction SilentlyContinue

Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('Desktop')) 'NewsQueue.lnk') -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\NewsQueue') -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NewsQueue' -Recurse -Force -ErrorAction SilentlyContinue

if (-not $silentUninstall) {
    [void]$shell.Popup('NewsQueue has been removed.', 0, 'Uninstall NewsQueue', 64)
}

$command = 'ping 127.0.0.1 -n 3 >nul & rmdir /s /q "' + $installDir + '"'
Start-Process -FilePath (Join-Path $env:SystemRoot 'System32\cmd.exe') -ArgumentList ('/c ' + $command) -WindowStyle Hidden
exit 0
