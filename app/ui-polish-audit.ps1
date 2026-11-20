# Hidden native UI matrix; isolated sample engine, no input injection.
param(
    [string]$Executable = "$PSScriptRoot\target\native-polished.exe",
    [string]$Output = "$PSScriptRoot\target\polish-audit\verified",
    [int[]]$Dpi = @(96, 120, 144, 192)
)
$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$names = @('NQ_DPI','NQ_CONFIG','NQ_DEMO_DIALOG','NQ_DEMO_TAB','NQ_DEMO_SETUP_PAGE','NQ_SCREEN')
$saved = @{}
foreach ($name in $names) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
}
$jobs = @()
try {
    foreach ($scale in $Dpi) {
        $env:NQ_DPI = "$scale"
        $env:NQ_CONFIG = Join-Path $Output "config-$scale"
        $folder = Join-Path $Output "dpi-$scale"
        $process = Start-Process -FilePath $Executable -ArgumentList @('--ui-audit', ('"'+$folder+'"')) -WindowStyle Hidden -PassThru -RedirectStandardOutput "$folder.out" -RedirectStandardError "$folder.err"
        # Cache the native handle before exit (Windows PowerShell 5.1 otherwise
        # sometimes returns a null ExitCode for an asynchronously started child).
        $null = $process.Handle
        $jobs += [pscustomobject]@{ Process=$process; Dpi=$scale; Folder=$folder }
    }
    foreach ($job in $jobs) {
        if (-not $job.Process.WaitForExit(300000)) { throw "DPI $($job.Dpi) audit timed out" }
        if ($null -eq $job.Process.ExitCode -or $job.Process.ExitCode -ne 0) { throw "DPI $($job.Dpi) audit failed: $($job.Process.ExitCode); see $($job.Folder).err" }
        $rows = Get-Content -LiteralPath (Join-Path $job.Folder 'controls.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        $issues = @($rows | ForEach-Object { $_.controls } | Where-Object { $_.issues.Count -gt 0 })
        "DPI $($job.Dpi): $($rows.Count) captures, $($issues.Count) layout warnings"
        if ($issues.Count) { throw "Review controls.json layout warnings at DPI $($job.Dpi)" }
    }
} finally {
    foreach ($job in $jobs) { if (-not $job.Process.HasExited) { $job.Process.Kill() } }
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
