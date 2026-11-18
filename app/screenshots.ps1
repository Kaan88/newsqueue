# Golden UI screenshots under app/shots/golden/ from the native window
# (PrintWindow, 100 % DPI, sample data, neutral C:\NewsQueue paths).
#
#   .\screenshots.ps1          regenerate the goldens (reports how much each changed)
#   .\screenshots.ps1 -Check   capture into shots\check\ and compare with the
#                              goldens; fails when an image differs beyond the
#                              tolerance (font smoothing and DPI cause small noise)
param([switch]$Check, [double]$TolerancePercent = 0.5)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public static class NqShotDiff {
    // Percentage of pixels whose largest channel difference exceeds 24, or
    // -1 when the sizes differ.
    public static double Percent(string a, string b) {
        using (var x = new Bitmap(a)) using (var y = new Bitmap(b)) {
            if (x.Width != y.Width || x.Height != y.Height) return -1;
            var r = new Rectangle(0, 0, x.Width, x.Height);
            var dx = x.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            var dy = y.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            int n = dx.Stride * x.Height;
            var px = new byte[n]; var py = new byte[n];
            Marshal.Copy(dx.Scan0, px, 0, n); Marshal.Copy(dy.Scan0, py, 0, n);
            x.UnlockBits(dx); y.UnlockBits(dy);
            long bad = 0;
            for (int i = 0; i < n; i += 4) {
                int d = Math.Max(Math.Abs(px[i] - py[i]), Math.Max(Math.Abs(px[i + 1] - py[i + 1]), Math.Abs(px[i + 2] - py[i + 2])));
                if (d > 24) bad++;
            }
            return 100.0 * bad / ((double)x.Width * x.Height);
        }
    }
}
'@
$vars = 'NQ_DEMO_TAB', 'NQ_DEMO_DIALOG', 'NQ_DEMO_SETUP_PAGE'
$outDir = if ($Check) { "shots\check" } else { "shots\golden" }
$results = @()
function Clear-Demo { foreach ($v in $vars) { Remove-Item "Env:$v" -ErrorAction SilentlyContinue } }
function Shot([string]$name) {
    $golden = "shots\golden\$name.png"
    $previous = $null
    if (-not $Check -and (Test-Path $golden)) {
        $previous = Join-Path $env:TEMP "nq-golden-prev-$name.png"
        Copy-Item $golden $previous -Force
    }
    & .\target\debug\newsqueue.exe --screenshot "$outDir\$name.png" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "screenshot $name failed" }
    $against = if ($Check) { $golden } else { $previous }
    $diff = if ($against -and (Test-Path $against)) { [NqShotDiff]::Percent((Resolve-Path "$outDir\$name.png"), (Resolve-Path $against)) } else { $null }
    if ($previous) { Remove-Item $previous -Force }
    $script:results += [pscustomobject]@{ Name = $name; Diff = $diff }
}
try {
    # cargo reports progress on stderr, which PowerShell 5.1 would treat as an error.
    $ErrorActionPreference = "Continue"
    cargo build -p newsqueue 2>&1 | Out-Null
    $ErrorActionPreference = "Stop"
    if ($LASTEXITCODE -ne 0) { throw 'build failed' }
    New-Item -ItemType Directory -Force $outDir | Out-Null
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
$failed = 0
foreach ($r in $results) {
    $text = if ($null -eq $r.Diff) { "new" } elseif ($r.Diff -lt 0) { "size changed" } else { "{0:N2} % of pixels differ" -f $r.Diff }
    $over = $null -ne $r.Diff -and ($r.Diff -lt 0 -or $r.Diff -gt $TolerancePercent)
    if ($over) { $failed++ }
    "{0,-28} {1}{2}" -f $r.Name, $text, $(if ($over) { "  <-- over $TolerancePercent %" } else { "" })
}
if ($Check) {
    if ($failed) { "SCREENSHOT CHECK FAILED: $failed of $($results.Count) differ"; exit 1 }
    "SCREENSHOT CHECK PASSED ($($results.Count) images within $TolerancePercent %)"
}
