param(
    [switch]$SkipPackage
)

# Builds dist\NewsQueue-<version>-Setup.exe (+ .sha256) with Inno Setup 7
# from the portable folder package.ps1 makes.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.10'
Push-Location $root
try {
    if (-not $SkipPackage) {
        & (Join-Path $root 'package.ps1')
    }

    $dist = Join-Path $root 'dist'
    $folder = Join-Path $dist "NewsQueue-$version"
    if (-not (Test-Path -LiteralPath (Join-Path $folder 'newsqueue.exe'))) {
        throw "Portable folder is missing: $folder (run package.ps1)"
    }

    $iscc = @(
        $env:NQ_ISCC,
        'C:\Program Files\Inno Setup 7\ISCC.exe',
        'C:\Program Files (x86)\Inno Setup 7\ISCC.exe',
        'C:\Program Files (x86)\Inno Setup 6\ISCC.exe'
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
    if (-not $iscc) {
        throw 'ISCC.exe not found: install Inno Setup 7 or set NQ_ISCC.'
    }

    $output = Join-Path $dist "NewsQueue-$version-Setup.exe"
    Remove-Item -LiteralPath $output -Force -ErrorAction SilentlyContinue
    & $iscc /Q "/DAppVersion=$version" "/DDistDir=$folder" "/DOutputDir=$dist" (Join-Path $root 'installer\newsqueue.iss')
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $output)) {
        throw "ISCC failed with exit code $LASTEXITCODE"
    }

    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash
    Set-Content -LiteralPath ($output + '.sha256') -Value ($hash + '  NewsQueue-' + $version + '-Setup.exe') -Encoding ascii

    Write-Output "Wrote $output"
    Write-Output "Size: $((Get-Item -LiteralPath $output).Length) bytes"
    Write-Output "SHA256: $hash"
} finally {
    Pop-Location
}
