fn main() {
    // `lse2` is an unstable target feature, so stable rustc never exposes
    // `cfg(target_feature = "lse2")`. Detect `+lse2` in the rustflags: the
    // 16-byte single-copy-atomic tagged-slot path requires it.
    println!("cargo:rustc-check-cfg=cfg(baml_lse2)");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let aarch64 = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|arch| arch == "aarch64");
    let lse2 = flags
        .split(['\x1f', ','])
        .any(|flag| flag.trim_start_matches("target-feature=") == "+lse2");
    if aarch64 && lse2 {
        println!("cargo:rustc-cfg=baml_lse2");
    }

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
