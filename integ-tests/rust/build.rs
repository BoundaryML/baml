fn main() {
    let cwd = std::env::current_dir().unwrap();
    // check for the debug or release directory
    // and use the one that was most recently modified

    let baml_library_path = {
        let (lib_prefix, lib_ext) = if cfg!(target_os = "windows") {
            ("", "dll")
        } else if cfg!(target_os = "macos") {
            ("lib", "dylib")
        } else {
            ("lib", "so")
        };

        let lib_name = format!("{lib_prefix}baml_cffi.{lib_ext}");
        let release_path = cwd.join(format!("../../engine/target/release/{}", lib_name));
        let debug_path = cwd.join(format!("../../engine/target/debug/{}", lib_name));

        // Get the most recently modified path
        [release_path.clone(), debug_path.clone()]
            .iter()
            .filter(|p| p.exists())
            .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "Neither release nor debug baml_cffi library found at:\n  {}\n  {}\n  Run `cargo build -p baml_cffi` from within engine",
                    release_path.display(),
                    debug_path.display()
                )
            })
    };

    // Add environment variable before running:
    baml_sys::set_library_path(baml_library_path.clone()).expect("Failed to set library path");
    baml_sys::ensure_library().expect("Failed to ensure library");

    // Embed into the binary
    println!("cargo:rustc-env=BAML_LIBRARY_PATH={baml_library_path:?}");
}
