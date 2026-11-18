fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/newsqueue-win32.manifest");
    println!("cargo:rerun-if-changed=newsqueue-win32.manifest");
}
