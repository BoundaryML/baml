fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    // One generated dispatch surface per stdlib package with Rust-implemented
    // builtins. Adding a `$rust_function` to a package's `.baml` source adds a
    // required trait method, so forgetting the Rust side is a compile error.
    for (package, file) in [
        ("baml", "nativefunctions_generated.rs"),
        ("ai", "aifunctions_generated.rs"),
        ("boundary", "boundaryfunctions_generated.rs"),
        ("reflect", "reflectfunctions_generated.rs"),
    ] {
        let (vm_builtins, _io_builtins, class_defs) =
            baml_builtins2_codegen::extract_native_builtins_for(package)
                .unwrap_or_else(|e| panic!("{e}"));
        let code =
            baml_builtins2_codegen::generate_native_trait_for(package, &vm_builtins, &class_defs);
        std::fs::write(format!("{out_dir}/{file}"), code).unwrap();
    }
}
