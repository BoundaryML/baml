//! Writes the traits the standard library natives implement and the symbols
//! compiled code calls them through, for the builtins listed in `natives/`.

use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    let lists = manifest_dir.join("natives");
    println!("cargo:rerun-if-changed={}", lists.display());

    // `cfg(checks)`: the runtime of a test build, and of any build with
    // debug assertions.
    println!("cargo::rustc-check-cfg=cfg(checks)");
    if env::var_os("CARGO_FEATURE_CHECKS").is_some()
        || env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some()
    {
        println!("cargo::rustc-cfg=checks");
    }

    let mut implemented =
        baml_builtins2_codegen::read_native_lists(&lists).unwrap_or_else(|error| panic!("{error}"));
    // A runtime built without `baml.http` has none of its natives.
    if env::var_os("CARGO_FEATURE_HTTP").is_none() {
        implemented.retain(|native| !native.path.starts_with("baml.http."));
    }
    let (vm, io, classes) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|error| panic!("{error}"));
    let shims = baml_builtins2_codegen::generate_native_shims(&vm, &io, &classes, &implemented)
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(out_dir.join("natives_generated.rs"), shims)
        .expect("the output directory is writable");
}
