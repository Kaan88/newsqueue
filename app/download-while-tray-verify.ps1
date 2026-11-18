$ErrorActionPreference = 'Stop'
# Downloads one article from a local mock NNTP server while the main window is
# hidden in the tray (SW_HIDE), then exits through the tray. Runs in a
# temporary config folder (NQ_CONFIG): the settings, queue and logs in app\
# are not touched.
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root 'target\debug\newsqueue.exe'
$config = Join-Path $env:TEMP ('newsqueue-tray-download-' + [Guid]::NewGuid().ToString('N'))
$settings = Join-Path $config 'newsqueue-settings.json'
$queue = Join-Path $config 'newsqueue-queue.json'
$outputDir = Join-Path $config 'download'
$mockScript = Join-Path $env:TEMP 'newsqueue-mock-nntp.py'
$port = Get-Random -Minimum 20000 -Maximum 40000
$proc = $null
if (-not (Test-Path $exe)) { throw 'Build the debug executable first.' }
if (Get-Process newsqueue -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }) { throw 'Close the debug NewsQueue first.' }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DownloadTrayCheck {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="FindWindowW")] public static extern IntPtr FindWindow(IntPtr cls, string title);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
}
"@

function Find-Own([string]$title) {
    $h = [DownloadTrayCheck]::FindWindow([IntPtr]::Zero, $title)
    if ($h -eq [IntPtr]::Zero) { return [IntPtr]::Zero }
    $owner = 0
    [void][DownloadTrayCheck]::GetWindowThreadProcessId($h, [ref]$owner)
    if ($owner -ne $proc.Id) { return [IntPtr]::Zero }
    return $h
}

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

try {
    New-Item -ItemType Directory -Force $outputDir | Out-Null
    $json = [ordered]@{
        servers = @([ordered]@{
            name = 'Mock NNTP'; address = '127.0.0.1'; port = $port; connections = 1
            ssl = $false; auth = $false; enabled = $true; username = ''; password = ''
        })
        download_dir = $outputDir
        nzb_dir = (Join-Path $config 'nzb')
        temp_dir = (Join-Path $config 'temp')
        unrar_dir = (Join-Path $config 'unrar')
        startup_connect = $true
        minimize_to_tray = $true
        minimize_to_tray_on_close = $true
        tray_balloon = $false
        log_enabled = $true
        log_to_file = $true
        par2_checking = $false
        auto_unrar = $false
        skip_existing = $false
        auto_import_folder = ''
        check_version_minutes = 0
    }
    [System.IO.File]::WriteAllText($settings, ($json | ConvertTo-Json -Depth 10), (New-Object System.Text.UTF8Encoding($false)))

    $queueItem = [ordered]@{
        id = 999; subject = 'Mock.Release'; total_bytes = 10; downloaded = 0.0; parts = 1; age_days = 0.0
        group = 'alt.binaries.test'; poster = 'tester'; status = 'Queued'; folder = $outputDir
        unrar_folder = ''; unrar = $false; execute = ''; execute_done = $false; expanded = $true
        files = @([ordered]@{
            name = 'mock.bin'; bytes = 10; downloaded = 0.0; parts = 1
            segments = @([ordered]@{ number = 1; bytes = 10; id = 'seg1' })
        })
        posted_on = '2026-11-04 00:00'; delete_archives = $false; password = ''; source = 'mock'
    }
    [System.IO.File]::WriteAllText($queue, '[' + ($queueItem | ConvertTo-Json -Depth 10) + ']', (New-Object System.Text.UTF8Encoding($false)))

    $logPath = Join-Path $config ('log\' + (Get-Date -Format 'yyyyMMdd') + '.log')
    $env:NQ_CONFIG = $config
    try { $proc = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru } finally { Remove-Item Env:NQ_CONFIG }

    $main = [IntPtr]::Zero
    for ($i = 0; $i -lt 100 -and $main -eq [IntPtr]::Zero; $i++) {
        Start-Sleep -Milliseconds 200
        $main = Find-Own 'NewsQueue 1.0.6'
    }
    if ($main -eq [IntPtr]::Zero) { throw 'main window not found' }
    Start-Sleep -Seconds 2

    # Minimize: with minimize_to_tray the window hides (SW_HIDE).
    [void][DownloadTrayCheck]::ShowWindow($main, 6)
    for ($i = 0; $i -lt 50 -and [DownloadTrayCheck]::IsWindowVisible($main); $i++) { Start-Sleep -Milliseconds 200 }
    if ([DownloadTrayCheck]::IsWindowVisible($main)) { throw 'window did not hide to the tray' }
    Write-Output 'hidden-in-tray=True'

    $completed = $false
    for ($i = 0; $i -lt 120; $i++) {
        Start-Sleep -Milliseconds 250
        if ((Test-Path -LiteralPath $logPath) -and ((Get-Content -LiteralPath $logPath) -match 'Download complete: Mock\.Release')) {
            $completed = $true
            break
        }
    }
    if (-not $completed) { throw 'download did not complete while hidden' }
    $stillHidden = -not [DownloadTrayCheck]::IsWindowVisible($main)
    Write-Output "download-complete-while-hidden=$completed stillHidden=$stillHidden"
    if (-not $stillHidden) { throw 'window became visible during the download' }

    $payload = Join-Path $outputDir 'mock.bin'
    if (-not (Test-Path -LiteralPath $payload)) { throw 'mock.bin was not created' }
    $content = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($payload))
    Write-Output "payload length=$($content.Length) content='$content'"
    if ($content -ne 'ABCDEFGHIJ') { throw 'unexpected mock payload' }

    $tray = Find-Own 'NewsQueue Tray'
    if ($tray -eq [IntPtr]::Zero) { throw 'tray window not found' }
    [void][DownloadTrayCheck]::PostMessage($tray, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero)
    if (-not $proc.WaitForExit(10000)) { throw 'tray Exit did not close the app' }
    Write-Output "exit-code=$($proc.ExitCode)"
    Write-Output 'DOWNLOAD-WHILE-TRAY PASSED'
} finally {
    if ($null -ne $proc -and -not $proc.HasExited) { $proc.Kill(); $proc.WaitForExit(5000) | Out-Null }
    if (-not $serverProcess.HasExited) { $serverProcess.Kill(); $serverProcess.WaitForExit(5000) | Out-Null }
    Remove-Item -LiteralPath $config -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $mockScript -Force -ErrorAction SilentlyContinue
}
