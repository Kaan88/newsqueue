# Regenerate the golden UI screenshots under app/shots/golden/ from the native
# window (PrintWindow, 100 % DPI, sample data, neutral C:\NewsQueue paths).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
$vars = 'NQ_DEMO_TAB', 'NQ_DEMO_DIALOG', 'NQ_DEMO_SETUP_PAGE'
function Clear-Demo { foreach ($v in $vars) { Remove-Item "Env:$v" -ErrorAction SilentlyContinue } }
function Shot([string]$name) {
    & .\target\debug\newsqueue.exe --screenshot "shots\golden\$name.png"
    if ($LASTEXITCODE -ne 0) { throw "screenshot $name failed" }
}
try {
    # cargo reports progress on stderr, which PowerShell 5.1 would treat as an error.
    $ErrorActionPreference = "Continue"
    cargo build -p newsqueue 2>&1 | Out-Null
    $ErrorActionPreference = "Stop"
    if ($LASTEXITCODE -ne 0) { throw 'build failed' }
    New-Item -ItemType Directory -Force "shots\golden" | Out-Null
    Clear-Demo
    Shot "queue"
    foreach ($tab in @("par2", "log", "rss")) {
        $env:NQ_DEMO_TAB = $tab
        Shot "tab-$tab"
    }
    Clear-Demo
    foreach ($dialog in @("setup", "server", "stats", "notes", "about", "columns", "pause", "speed", "version",
                          "shutdown", "unrar-password", "rename", "execute", "rss-feeds")) {
        $env:NQ_DEMO_DIALOG = $dialog
        Shot "dialog-$dialog"
    }
    # Setup pages with more than plain fields.
    $env:NQ_DEMO_DIALOG = "setup"
    foreach ($page in @(@(4, "servers"), @(8, "scheduler"), @(10, "unpack"))) {
        $env:NQ_DEMO_SETUP_PAGE = "$($page[0])"
        Shot "dialog-setup-$($page[1])"
    }
} finally {
    Clear-Demo
    Pop-Location
}
