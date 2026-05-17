use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // === cbindgen: emit inputx_core.h ===
    let header_dir = crate_dir.join("../../include");
    std::fs::create_dir_all(&header_dir).unwrap();

    let cbindgen_config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("cbindgen.toml not found");

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cbindgen_config)
        .generate()
        .expect("cbindgen failed")
        .write_to_file(header_dir.join("inputx_core.h"));

    // Wubi data layer is now provided by the `wubi` crate dep — no dict
    // file generation needed here.

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=build.rs");
}
