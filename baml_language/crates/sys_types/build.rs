fn main() {
    let (_vm_builtins, io_builtins, class_defs) = baml_builtins2_codegen::extract_native_builtins()
        .expect("failed to extract builtins from BAML stdlib");

    let code = baml_builtins2_codegen::generate_io_structs(&io_builtins, &class_defs);
    let runtime_io_code =
        baml_builtins2_codegen::generate_runtime_io(&io_builtins, &class_defs, "super::generated");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out_dir}/io_generated.rs"), code)
        .expect("failed to write io_generated.rs");
    std::fs::write(format!("{out_dir}/runtime_io.rs"), runtime_io_code)
        .expect("failed to write runtime_io.rs");
}
