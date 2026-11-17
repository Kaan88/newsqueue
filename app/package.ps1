# Build a self-contained portable NewsQueue folder under app/dist/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.3'
Push-Location $root
try {
    cargo build --release
    $dist = Join-Path $root "dist\NewsQueue-$version"
    Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item "target\release\newsqueue.exe" $dist
    Copy-Item "tools" (Join-Path $dist "tools") -Recurse
    Copy-Item "assets\newsqueue-256.png" $dist
    Copy-Item "..\README.md" $dist
    Copy-Item "..\ROADMAP.md" $dist
    Compress-Archive -Path $dist -DestinationPath (Join-Path $root "dist\NewsQueue-$version.zip") -Force
    Write-Output "Wrote $dist"
    Write-Output "Wrote $root\dist\NewsQueue-$version.zip"
} finally {
    Pop-Location
}
