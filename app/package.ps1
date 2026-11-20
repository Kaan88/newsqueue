# Build a self-contained portable NewsQueue folder under app/dist/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.8'
Push-Location $root
try {
    cargo build --release --locked --offline
    if ($LASTEXITCODE -ne 0) { throw "Release build failed: $LASTEXITCODE" }
    $dist = Join-Path $root "dist\NewsQueue-$version"
    $dist = [IO.Path]::GetFullPath($dist)
    $expected = [IO.Path]::GetFullPath((Join-Path $root "dist")) + [IO.Path]::DirectorySeparatorChar
    if (-not $dist.StartsWith($expected, [StringComparison]::OrdinalIgnoreCase)) { throw "Invalid package path: $dist" }
    Remove-Item -LiteralPath $dist -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item "target\release\newsqueue.exe" $dist
    Copy-Item "tools" (Join-Path $dist "tools") -Recurse
    Copy-Item "assets\newsqueue-256.png" $dist
    Copy-Item "..\README.md" $dist
    Copy-Item "..\ROADMAP.md" $dist
    # The egui web UI for the "Web access" setting, if it has been built
    # (web\build-web.ps1).
    if (Test-Path "web\dist\index.html") { Copy-Item "web\dist" (Join-Path $dist "web") -Recurse }
    Compress-Archive -Path $dist -DestinationPath (Join-Path $root "dist\NewsQueue-$version.zip") -Force
    Write-Output "Wrote $dist"
    Write-Output "Wrote $root\dist\NewsQueue-$version.zip"
} finally {
    Pop-Location
}
