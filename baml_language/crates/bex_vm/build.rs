fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    baml_builtins2_codegen::generate(
        baml_builtins2_codegen::BuiltinCrate::BexVm,
        std::path::Path::new(&out_dir),
    )
}
