fn main() {
    embed_resource::compile("mockup.rc", embed_resource::NONE);
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/../newsqueue.manifest");
    println!("cargo:rerun-if-changed=../newsqueue.manifest");
    println!("cargo:rerun-if-changed=../assets/newsqueue.ico");
}
