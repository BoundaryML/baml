fn main() {
    let (_vm_builtins, io_builtins, class_defs) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|e| panic!("{e}"));
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let code = baml_builtins2_codegen::generate_sys_op_enum(&io_builtins);
    std::fs::write(format!("{out_dir}/sys_op_generated.rs"), code).unwrap();

    let error_code = baml_builtins2_codegen::generate_error_enums(&class_defs);
    std::fs::write(format!("{out_dir}/errors_generated.rs"), error_code).unwrap();

    let panic_code = baml_builtins2_codegen::generate_panic_enums(&class_defs);
    std::fs::write(format!("{out_dir}/panics_generated.rs"), panic_code).unwrap();
}
