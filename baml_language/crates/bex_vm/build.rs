fn main() {
    let (vm_builtins, _io_builtins, class_defs) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|e| panic!("{e}"));
    let code = baml_builtins2_codegen::generate_native_trait(&vm_builtins, &class_defs);
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out_dir}/nativefunctions_generated.rs"), code).unwrap();
}
