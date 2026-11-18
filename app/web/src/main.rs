//! NewsQueue web UI: the egui front end in the browser (GUI_PLAN Phase 1).
//!
//! Compiled to WebAssembly and served by `newsqueue-server` at `/`. It holds
//! no engine: it polls `GET /api/state` (the secret-free `Snapshot`) and
//! sends actions to the JSON API. The look is a classic Windows 2000 desktop
//! window (`skin.rs` draws the widgets, `theme.rs` the palette and icons).
#![allow(dead_code)]

mod skin;
mod theme;
mod app;
mod net;
mod settings;

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("nq"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("canvas #nq");
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::WebApp::new(cc)))),
            )
            .await;
        if let Err(e) = result {
            if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("loading")) {
                el.set_text_content(Some(&format!("Failed to start: {e:?}")));
            }
        }
    });
}

/// Native builds exist only so `cargo check --workspace` passes.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("newsqueue-web runs in the browser: build it with web\\build-web.ps1.");
}
