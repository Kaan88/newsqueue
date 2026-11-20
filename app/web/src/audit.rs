//! Read-only widget bounds for headless QA; absent from production builds.
#[cfg(all(feature = "ui-audit", target_arch = "wasm32"))]
thread_local! { static ITEMS: std::cell::RefCell<Vec<serde_json::Value>> = const { std::cell::RefCell::new(Vec::new()) }; }
pub fn begin() {
    #[cfg(all(feature = "ui-audit", target_arch = "wasm32"))]
    ITEMS.with(|v| v.borrow_mut().clear());
}
pub fn item(ui: &egui::Ui, label: &str, rect: egui::Rect) {
    #[cfg(all(feature = "ui-audit", target_arch = "wasm32"))]
    if ui.is_rect_visible(rect) {
        let clip = rect.intersect(ui.clip_rect());
        ITEMS.with(|v|v.borrow_mut().push(serde_json::json!({"label":label,"rect":[rect.min.x,rect.min.y,rect.max.x,rect.max.y],"clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y],"enabled":ui.is_enabled()})));
    }
    let _ = (ui, label, rect);
}
pub fn finish() {
    #[cfg(all(feature = "ui-audit", target_arch = "wasm32"))]
    if let Some(window) = web_sys::window() {
        let json = ITEMS.with(|v| serde_json::to_string(&*v.borrow()).unwrap());
        let _ = js_sys::Reflect::set(
            &window,
            &"__nqAudit".into(),
            &js_sys::JSON::parse(&json).unwrap(),
        );
    }
}

pub fn state(snap: &crate::net::Snapshot) {
    #[cfg(all(feature = "ui-audit", target_arch = "wasm32"))]
    if let Some(window) = web_sys::window() {
        let value = serde_json::json!({"paused":snap.paused,"queue":snap.queue.iter().map(|q|serde_json::json!({"id":q.id,"subject":q.subject,"status":q.status,"expanded":q.expanded})).collect::<Vec<_>>()});
        let _ = js_sys::Reflect::set(
            &window,
            &"__nqState".into(),
            &js_sys::JSON::parse(&value.to_string()).unwrap(),
        );
    }
    let _ = snap;
}
