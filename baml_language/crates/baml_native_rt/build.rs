//! Writes the traits the standard library natives implement and the symbols
//! compiled code calls them through, for the builtins listed in `natives/`.

use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    let lists = manifest_dir.join("natives");
    println!("cargo:rerun-if-changed={}", lists.display());

    let implemented =
        baml_builtins2_codegen::read_native_lists(&lists).unwrap_or_else(|error| panic!("{error}"));
    let (vm, io, classes) =
        baml_builtins2_codegen::extract_native_builtins().unwrap_or_else(|error| panic!("{error}"));
    let shims = baml_builtins2_codegen::generate_native_shims(&vm, &io, &classes, &implemented)
        .unwrap_or_else(|error| panic!("{error}"));
    std::fs::write(out_dir.join("natives_generated.rs"), shims)
        .expect("the output directory is writable");
}
