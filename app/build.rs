fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("app.rc", embed_resource::NONE);
        // Common controls v6 and per-monitor v2 DPI awareness.
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/newsqueue.manifest");
        println!("cargo:rerun-if-changed=newsqueue.manifest");
        println!("cargo:rerun-if-changed=app.rc");
    }
}
