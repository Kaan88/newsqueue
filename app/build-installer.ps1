param(
    [switch]$SkipPackage
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$version = '1.0.1'
Push-Location $root
try {
    if (-not $SkipPackage) {
        & (Join-Path $root 'package.ps1')
    }

    $dist = Join-Path $root 'dist'
    $zip = Join-Path $dist "NewsQueue-$version.zip"
    if (-not (Test-Path -LiteralPath $zip)) {
        throw "Portable package is missing: $zip"
    }

    $stage = Join-Path $env:TEMP "newsqueue-setup-$version"
    if ($stage -match ' ') {
        throw "IExpress cannot handle spaces in the staging path: $stage"
    }
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $stage | Out-Null

    Copy-Item -LiteralPath $zip -Destination (Join-Path $stage "NewsQueue-$version.zip") -Force
    Copy-Item -LiteralPath (Join-Path $root 'installer\install.ps1') -Destination $stage -Force
    Copy-Item -LiteralPath (Join-Path $root 'installer\install.cmd') -Destination $stage -Force
    Copy-Item -LiteralPath (Join-Path $root 'installer\uninstall.ps1') -Destination $stage -Force

    $target = Join-Path $stage "NewsQueue-$version-Setup.exe"
    $sedPath = Join-Path $stage 'setup.sed'
    $sed = @"
[Version]
Class=IEXPRESS
SEDVersion=3
[Options]
PackagePurpose=InstallApp
ShowInstallProgramWindow=1
HideExtractAnimation=1
UseLongFileName=1
InsideCompressed=0
CAB_FixedSize=0
CAB_ResvCodeSigning=0
RebootMode=N
InstallPrompt=
DisplayLicense=
FinishMessage=
TargetName=$target
FriendlyName=NewsQueue $version Setup
AppLaunched=cmd.exe /c install.cmd
PostInstallCmd=<None>
AdminQuietInstCmd=
UserQuietInstCmd=
SourceFiles=SourceFiles
[Strings]
FILE0="NewsQueue-$version.zip"
FILE1="install.cmd"
FILE2="install.ps1"
FILE3="uninstall.ps1"
[SourceFiles]
SourceFiles0=$stage\
[SourceFiles0]
%FILE0%=
%FILE1%=
%FILE2%=
%FILE3%=
"@
    Set-Content -LiteralPath $sedPath -Value $sed -Encoding ascii
    Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
    & (Join-Path $env:SystemRoot 'System32\iexpress.exe') /N /Q $sedPath

    $ready = $false
    for ($i = 0; $i -lt 120; $i++) {
        Start-Sleep -Milliseconds 500
        if ((Test-Path -LiteralPath $target) -and ((Get-Item -LiteralPath $target).Length -gt 0)) {
            $ready = $true
            break
        }
    }
    if (-not $ready) {
        throw 'IExpress did not produce the setup executable.'
    }
    Start-Sleep -Milliseconds 500

    $output = Join-Path $dist "NewsQueue-$version-Setup.exe"
    Copy-Item -LiteralPath $target -Destination $output -Force
    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash
    Set-Content -LiteralPath ($output + '.sha256') -Value ($hash + '  NewsQueue-' + $version + '-Setup.exe') -Encoding ascii

    Write-Output "Wrote $output"
    Write-Output "Size: $((Get-Item -LiteralPath $output).Length) bytes"
    Write-Output "SHA256: $hash"
    Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
} finally {
    Pop-Location
}
