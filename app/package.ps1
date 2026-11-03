# Build a self-contained portable NewsQueue folder under app/dist/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
try {
    cargo build --release
    $dist = Join-Path $root "dist\NewsQueue-0.48.4"
    Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $dist | Out-Null
    Copy-Item "target\release\newsqueue.exe" $dist
    Copy-Item "tools" (Join-Path $dist "tools") -Recurse
    Copy-Item "assets\newsqueue-256.png" $dist
    Copy-Item "..\README.md" $dist
    Copy-Item "..\ROADMAP.md" $dist
    Compress-Archive -Path $dist -DestinationPath (Join-Path $root "dist\NewsQueue-0.48.4.zip") -Force
    Write-Output "Wrote $dist"
    Write-Output "Wrote $root\dist\NewsQueue-0.48.4.zip"
} finally {
    Pop-Location
}
