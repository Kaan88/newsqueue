# Regenerate the golden UI screenshots under app/shots/golden/.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
try {
    New-Item -ItemType Directory -Force "shots\golden" | Out-Null
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_DIALOG -ErrorAction SilentlyContinue
    cargo run -- --screenshot "shots\golden\queue.png"
    foreach ($tab in @("par2", "log", "rss")) {
        $env:NQ_DEMO_TAB = $tab
        cargo run -- --screenshot "shots\golden\tab-$tab.png"
    }
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    foreach ($dialog in @("setup", "server", "stats", "notes", "about", "columns", "pause", "shutdown", "unrar-password")) {
        $env:NQ_DEMO_DIALOG = $dialog
        cargo run -- --screenshot "shots\golden\dialog-$dialog.png"
    }
    # The light variant (screenshots are dark unless NQ_DEMO_THEME says otherwise).
    $env:NQ_DEMO_THEME = "light"
    cargo run -- --screenshot "shots\golden\queue-light.png"
    $env:NQ_DEMO_DIALOG = "setup"
    cargo run -- --screenshot "shots\golden\dialog-setup-light.png"
    Remove-Item Env:NQ_DEMO_DIALOG -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_THEME -ErrorAction SilentlyContinue
} finally {
    Remove-Item Env:NQ_DEMO_TAB -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_DIALOG -ErrorAction SilentlyContinue
    Remove-Item Env:NQ_DEMO_THEME -ErrorAction SilentlyContinue
    Pop-Location
}
