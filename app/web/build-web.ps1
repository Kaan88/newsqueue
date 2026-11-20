# Builds the egui web UI into web\dist (served by newsqueue-server at /).
# Needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version 0.2.130
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo build -p newsqueue-web --release --target wasm32-unknown-unknown
if (-not $?) { exit 1 }
$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Force (Join-Path $dist 'pkg') | Out-Null
wasm-bindgen --target web --no-typescript --out-dir (Join-Path $dist 'pkg') target\wasm32-unknown-unknown\release\newsqueue-web.wasm
if (-not $?) { exit 1 }
Copy-Item (Join-Path $PSScriptRoot 'index.html') $dist -Force
Copy-Item (Join-Path $PSScriptRoot '../assets/fonts/*LICENSE.txt') $dist -Force
Get-ChildItem -Recurse $dist | Select-Object FullName, Length
