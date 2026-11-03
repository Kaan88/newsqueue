$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
$settings = Join-Path $root 'newsqueue-settings.json'
$queue = Join-Path $root 'newsqueue-queue.json'
$history = Join-Path $root 'newsqueue-history.json'
$stats = Join-Path $root 'newsqueue-stats.json'
$outputDir = Join-Path $env:TEMP 'newsqueue-offscreen-download'
$mockScript = Join-Path $env:TEMP 'newsqueue-mock-nntp.py'
$port = Get-Random -Minimum 20000 -Maximum 40000
$proc = $null
if (-not (Test-Path $exe)) { throw 'Build the debug executable first.' }
if (Get-Process newsqueue -ErrorAction SilentlyContinue) { throw 'Close NewsQueue first.' }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public struct RECTD { public int Left, Top, Right, Bottom; }
public class DownloadTrayCheck {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECTD r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
}
"@

$python = @'
import socket, threading, time

BODY = (b"=ybegin line=128 size=10 name=mock.bin\n"
        b"klmnopqrst\n"
        b"=yend size=10\n")

def handle(conn):
    try:
        f = conn.makefile('rb')
        conn.sendall(b"200 mock ready\r\n")
        while True:
            line = f.readline()
            if not line:
                break
            command = line.strip()
            if command == b"QUIT":
                break
            if command.startswith(b"BODY <"):
                time.sleep(8)
                conn.sendall(b"222 0 mock\r\n" + BODY + b"\r\n.\r\n")
            elif command.startswith(b"AUTHINFO"):
                conn.sendall(b"281 mock auth\r\n")
            else:
                conn.sendall(b"500 mock unsupported\r\n")
    finally:
        conn.close()

server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", 18766))
server.listen(8)
while True:
    conn, _ = server.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
'@
Set-Content -LiteralPath $mockScript -Value $python.Replace('18766', "$port") -Encoding ascii
$serverProcess = Start-Process -FilePath (Get-Command python).Source -ArgumentList $mockScript -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 1

$backup = Join-Path $env:TEMP ('newsqueue-download-backup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $backup | Out-Null
foreach ($file in @($settings, $queue, $history, $stats)) {
    if (Test-Path -LiteralPath $file) { Copy-Item -LiteralPath $file -Destination $backup -Force }
}

try {
    Remove-Item -LiteralPath $outputDir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $outputDir | Out-Null

    $json = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
    $server = $json.servers[0]
    $server.name = 'Mock NNTP'
    $server.address = '127.0.0.1'
    $server.port = $port
    $server.connections = 1
    $server.ssl = $false
    $server.auth = $false
    $server.enabled = $true
    $json.servers = @($server)
    $json.download_dir = $outputDir
    $json.nzb_dir = (Join-Path $outputDir 'nzb')
    $json.temp_dir = (Join-Path $outputDir 'temp')
    $json.unrar_dir = (Join-Path $outputDir 'unrar')
    $json.startup_connect = $true
    $json.minimize_to_tray = $true
    $json.minimize_to_tray_on_close = $true
    $json.tray_balloon = $false
    $json.log_enabled = $true
    $json.log_to_file = $true
    $json.par2_checking = $false
    $json.auto_unrar = $false
    $json.skip_existing = $false
    $json.auto_import_folder = ''
    $settingsText = $json | ConvertTo-Json -Depth 30
    [System.IO.File]::WriteAllText($settings, $settingsText, (New-Object System.Text.UTF8Encoding($false)))

    $queueItem = [pscustomobject]@{
        id = 999
        subject = 'Mock.Release'
        total_bytes = 10
        downloaded = 0.0
        parts = 1
        age_days = 0.0
        group = 'alt.binaries.test'
        poster = 'tester'
        status = 'Queued'
        folder = $outputDir
        unrar_folder = ''
        unrar = $false
        execute = ''
        execute_done = $false
        expanded = $true
        files = @([pscustomobject]@{
            name = 'mock.bin'
            bytes = 10
            downloaded = 0.0
            parts = 1
            segments = @([pscustomobject]@{ number = 1; bytes = 10; id = 'seg1' })
        })
        posted_on = '2026-11-04 00:00'
        delete_archives = $false
        category = 'Default'
        password = ''
        source = 'mock'
    }
    $queueText = '[' + ($queueItem | ConvertTo-Json -Depth 10) + ']'
    [System.IO.File]::WriteAllText($queue, $queueText, (New-Object System.Text.UTF8Encoding($false)))

    $logPath = Join-Path $root ('log\' + (Get-Date -Format 'yyyyMMdd') + '.log')
    $logStart = if (Test-Path $logPath) { (Get-Content -LiteralPath $logPath).Count } else { 0 }

    $proc = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru
    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100; $i++) {
        Start-Sleep -Milliseconds 200
        $main = [DownloadTrayCheck]::FindWindow([IntPtr]::Zero, 'NewsQueue 1.0.0')
        if ($main -ne [IntPtr]::Zero) { break }
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    Start-Sleep -Seconds 2
    [void][DownloadTrayCheck]::ShowWindow($main, 6)
    $rect = New-Object RECTD
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 200
        [void][DownloadTrayCheck]::GetWindowRect($main, [ref]$rect)
        if ($rect.Left -le -10000) { break }
    }
    if ($rect.Left -gt -10000) { throw 'window did not move off-screen' }
    Write-Output "offscreen rect=$($rect.Left),$($rect.Top),$($rect.Right),$($rect.Bottom)"

    $completed = $false
    for ($i = 0; $i -lt 120; $i++) {
        Start-Sleep -Milliseconds 250
        if (Test-Path -LiteralPath $logPath) {
            $newLines = Get-Content -LiteralPath $logPath | Select-Object -Skip $logStart
            if ($newLines -match 'Download complete: Mock\.Release') {
                $completed = $true
                break
            }
        }
    }
    if (-not $completed) { throw 'download did not complete while off-screen' }
    [void][DownloadTrayCheck]::GetWindowRect($main, [ref]$rect)
    $stillOffscreen = $rect.Left -le -10000
    Write-Output "download-complete-while-offscreen=$completed stillOffscreen=$stillOffscreen"

    $payload = Join-Path $outputDir 'mock.bin'
    if (-not (Test-Path -LiteralPath $payload)) { throw 'mock.bin was not created' }
    $content = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($payload))
    Write-Output "payload length=$($content.Length) content='$content'"
    if ($content -ne 'ABCDEFGHIJ') { throw 'unexpected mock payload' }

    $tray = [DownloadTrayCheck]::FindWindow([IntPtr]::Zero, 'NewsQueue Tray')
    if ($tray -ne [IntPtr]::Zero) {
        [void][DownloadTrayCheck]::PostMessage($tray, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero)
        [void]$proc.WaitForExit(10000)
    }
    if (-not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
    Write-Output 'DOWNLOAD-WHILE-OFFSCREEN PASSED'
} finally {
    if ($null -ne $proc -and -not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
    if (-not $serverProcess.HasExited) { $serverProcess.Kill(); $serverProcess.WaitForExit(5000) | Out-Null }
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
    Remove-Item -LiteralPath $outputDir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $mockScript -Force -ErrorAction SilentlyContinue
}
