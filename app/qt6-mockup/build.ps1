[CmdletBinding()]
param(
    [string] $QtRoot = 'C:\Qt\6.9.3\msvc2022_64'
)

$ErrorActionPreference = 'Stop'
$target = Join-Path $PSScriptRoot '..\target\qt6-mockup'
$build = Join-Path $target 'build'
$preview = Join-Path $target 'preview'
$deploy = Join-Path $QtRoot 'bin\windeployqt.exe'
if (-not (Test-Path -LiteralPath $deploy -PathType Leaf)) {
    throw "Qt 6 MSVC kit not found at $QtRoot. Pass its path with -QtRoot."
}

cmake -S $PSScriptRoot -B $build -A x64 "-DCMAKE_PREFIX_PATH=$QtRoot"
if ($LASTEXITCODE -ne 0) { throw "CMake configuration failed ($LASTEXITCODE)." }
cmake --build $build --config Release --parallel
if ($LASTEXITCODE -ne 0) { throw "Compilation failed ($LASTEXITCODE)." }

New-Item -ItemType Directory -Force -Path $preview | Out-Null
$exe = Join-Path $preview 'newsqueue-qt-compact.exe'
Copy-Item -LiteralPath (Join-Path $build 'Release\newsqueue-qt-compact.exe') -Destination $exe
& $deploy --release --no-translations --no-system-d3d-compiler --no-opengl-sw --no-compiler-runtime --include-plugins qoffscreen $exe
if ($LASTEXITCODE -ne 0) { throw "Qt deployment failed ($LASTEXITCODE)." }
Write-Output (Resolve-Path -LiteralPath $exe).Path
