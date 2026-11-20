# Hidden web QA

Run from the repository root using PowerShell. These scripts launch only headless Chrome and their own isolated demo server, and clean up their child processes. They never drive the desktop. Chrome is expected at its standard Program Files location.

Dependencies: Python, Playwright 1.65.2, the Rust wasm32 target, and wasm-bindgen CLI matching Cargo.lock (0.2.130). Install Playwright locally with `python -m pip install --target app/target/qa-python playwright==1.65.2`; no browser download is needed.

Prepare the instrumented build:

```powershell
$env:RUSTC_WRAPPER = ''
cargo build --release --locked --offline --manifest-path app/Cargo.toml -p newsqueue-server
cargo build --release --locked --offline --manifest-path app/Cargo.toml -p newsqueue-web --target wasm32-unknown-unknown --features ui-audit
New-Item -ItemType Directory -Force app/target/web-audit/save-fixed/pkg
wasm-bindgen --target web --no-typescript --out-dir app/target/web-audit/save-fixed/pkg app/target/wasm32-unknown-unknown/release/newsqueue-web.wasm
Copy-Item app/web/index.html app/target/web-audit/save-fixed/index.html
python app/web/qa/functionality.py
python app/web/qa/admin.py
python app/web/qa/postpass.py
```

`states.py` expects the same instrumented build at `target/web-audit/final`. The full `headless.py` matrix accepts `NQ_AUDIT_BUILD`, `NQ_AUDIT_RUN`, `NQ_AUDIT_PORT`, and `NQ_AUDIT_THEMES`. Use `NQ_AUDIT_BUILD=save-fixed` and a fresh run name; existing PASS markers skip completed cases. Classic baseline comparison additionally requires the pre-change 1.0.8 web build in `target/web-audit/classic-baseline`. Preserve/copy that build before rebuilding it; do not substitute the current build as its own baseline.

The `ui-audit` feature exposes read-only widget bounds and selected public queue state to the harness; input still goes through real browser mouse/keyboard events and real egui controls. Never deploy this diagnostic build. Rebuild production separately with `app/web/build-web.ps1` (no feature flag). See `../QA_REPORT.md` for results and limits.
