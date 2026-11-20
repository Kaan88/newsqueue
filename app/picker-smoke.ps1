[CmdletBinding()]
param(
    [ValidateSet('Prepare', 'Verify')][string]$Mode = 'Prepare',
    [string]$Exe,
    [string]$Directory
)
$ErrorActionPreference = 'Stop'
if ($Mode -eq 'Prepare') {
    if (-not $Exe) { throw 'Prepare requires -Exe pointing to the build to test.' }
    $sourceExe = (Resolve-Path -LiteralPath $Exe).Path
    if (-not $Directory) {
        $Directory = Join-Path $PSScriptRoot ('target\picker-smoke-' + [guid]::NewGuid().ToString('N'))
    }
    if (Test-Path -LiteralPath $Directory) { throw 'Prepare requires a new directory; existing state is never overwritten.' }
    New-Item -ItemType Directory -Path $Directory | Out-Null
    $Directory = (Resolve-Path -LiteralPath $Directory).Path
    Copy-Item -LiteralPath $sourceExe -Destination (Join-Path $Directory 'newsqueue.exe')
    $settings = @{
        startup_connect = $false; check_version_minutes = 0; auto_update = $false
        servers = @(); rss_urls = @(); web_access = $false
        minimize_to_tray_on_close = $false; auto_import_folder = ''
        download_dir = (Join-Path $Directory 'download')
        nzb_dir = (Join-Path $Directory 'nzb'); temp_dir = (Join-Path $Directory 'temp')
        log_to_file = $true
    } | ConvertTo-Json
    [IO.File]::WriteAllText((Join-Path $Directory 'newsqueue-settings.json'), $settings)
    New-Item -ItemType Directory -Path (Join-Path $Directory 'download') | Out-Null
    foreach ($name in @('toolbar', 'menu', 'paused', 'multi-one', 'multi-two')) {
        $xml = '<?xml version="1.0"?><nzb xmlns="http://www.newzbin.com/DTD/2003/nzb"><file poster="audit@example.invalid" date="1795132800" subject="&quot;{0}.bin&quot; yEnc (1/1)"><groups><group>alt.test</group></groups><segments><segment bytes="128" number="1">{0}@example.invalid</segment></segments></file></nzb>' -f $name
        [IO.File]::WriteAllText((Join-Path $Directory "$name.nzb"), $xml)
    }
    [IO.File]::WriteAllText((Join-Path $Directory 'invalid.nzb'), 'not an NZB file')
    Write-Output "Prepared isolated test folder: $Directory"
    Write-Output 'No application launched. Follow PICKER_SMOKE.md only when interactive testing is requested.'
    exit 0
}
if (-not $Directory) { throw 'Verify requires -Directory from a completed smoke run.' }
$Directory = (Resolve-Path -LiteralPath $Directory).Path
$queue = @(Get-Content -Raw -LiteralPath (Join-Path $Directory 'newsqueue-queue.json') | ConvertFrom-Json)
$expected = @{toolbar='Queued'; menu='Queued'; paused='Paused'; 'multi-one'='Queued'; 'multi-two'='Queued'}
if ($queue.Count -ne $expected.Count) { throw "Expected 5 persisted collections, got $($queue.Count)." }
foreach ($name in $expected.Keys) {
    $items = @($queue | Where-Object subject -eq $name)
    if ($items.Count -ne 1 -or $items[0].status -ne $expected[$name]) { throw "Wrong persisted result for $name." }
    if ($items[0].files[0].segments[0].id -ne "$name@example.invalid") { throw "Missing segment ID for $name." }
}
$toolbar = @($queue | Where-Object subject -eq 'toolbar')[0]
if ($toolbar.unrar_folder -ne (Join-Path $Directory 'download')) { throw 'Folder picker result was not persisted.' }
[xml]$export = Get-Content -Raw -LiteralPath (Join-Path $Directory 'exported-toolbar.nzb')
if ($export.nzb.file.segments.segment.InnerText -ne 'toolbar@example.invalid') { throw 'Export did not preserve the article ID.' }
$logs = Get-ChildItem -LiteralPath (Join-Path $Directory 'log') -Filter '*.log' | ForEach-Object { Get-Content -LiteralPath $_.FullName }
if (-not ($logs | Select-String -SimpleMatch 'No files found in NZB')) { throw 'Invalid NZB rejection was not recorded.' }
Write-Output 'PASS: persisted imports, paused state, segment IDs, folder selection, export and invalid-file rejection.'
Write-Output 'This checks artifacts; record observed toolbar/menu/shortcut/cancel/reload interactions separately.'
