fn main() {
    let builtins = baml_builtins2_codegen::extract_native_builtins();
    let code = baml_builtins2_codegen::generate_native_trait(&builtins);
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out_dir}/nativefunctions_generated.rs"), code).unwrap();
}
