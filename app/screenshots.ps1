# Regenerate the golden UI screenshots under app/shots/golden/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
try {
    New-Item -ItemType Directory -Force "shots\golden" | Out-Null
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_DIALOG -ErrorAction SilentlyContinue
    cargo run -- --screenshot "shots\golden\queue.png"
    foreach ($tab in @("par2", "explorer", "log", "rss")) {
        $env:NQ_DEMO_TAB = $tab
        cargo run -- --screenshot "shots\golden\tab-$tab.png"
    }
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    foreach ($dialog in @("setup", "server", "stats", "categories", "notes", "about", "columns", "pause", "shutdown", "unrar-password")) {
        $env:NQ_DEMO_DIALOG = $dialog
        cargo run -- --screenshot "shots\golden\dialog-$dialog.png"
    }
} finally {
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_DIALOG -ErrorAction SilentlyContinue
    Pop-Location
}
