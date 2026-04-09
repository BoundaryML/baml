fn main() {
    let (_vm_builtins, io_builtins, class_defs) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|e| panic!("{e}"));
    let code = baml_builtins2_codegen::generate_io_traits(
        &io_builtins,
        &class_defs,
        "sys_types::generated",
    );
    let adapter_code = baml_builtins2_codegen::generate_io_adapter(
        &io_builtins,
        &class_defs,
        "sys_types::generated",
    );
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out_dir}/io_generated.rs"), code).unwrap();
    std::fs::write(format!("{out_dir}/io_adapter.rs"), adapter_code).unwrap();
}
