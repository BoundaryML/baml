fn main() {
    // Verify we're building for the correct target
    let target = std::env::var("TARGET").unwrap();
    if target != "x86_64-unknown-linux-gnu" {
        // Don't fail - this crate just won't provide symbols for other targets
        // The correct platform-specific crate will be used instead
        return;
    }

    let lib_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lib");

    // Only emit link directives if the library exists
    // During development, the library might not be built yet
    let lib_path = lib_dir.join("libbaml_cffi.a");
    if lib_path.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=baml_cffi");
    } else {
        println!(
            "cargo:warning=libbaml_cffi.a not found at {}. FFI calls will fail at link time.",
            lib_path.display()
        );
    }
}
