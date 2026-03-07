fn main() {
    baml_builtins_codegen::validate_compiler2_stdlib()
        .expect("compiler2 builtin stdlib validation failed");

    let input = include_str!("legacy_projection.dsl");
    let generated =
        baml_builtins_codegen::generate_module(input).expect("failed to generate builtin code");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = std::path::Path::new(&out_dir).join("builtins_generated.rs");
    std::fs::write(&out_path, generated).expect("failed to write builtin generated code");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=legacy_projection.dsl");
    println!("cargo:rerun-if-changed=../baml_builtins2/baml_std");
}
