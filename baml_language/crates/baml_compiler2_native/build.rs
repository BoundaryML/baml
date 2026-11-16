//! Compiles the runtime that native programs link against into a static
//! archive, and tells the crate where it is and which system libraries it
//! needs at link time.
//!
//! The runtime is one dependency-free source file, so it is compiled with a
//! direct `rustc` invocation rather than a nested `cargo`.

use std::{env, path::PathBuf, process::Command};

/// The line `rustc --print native-static-libs` reports the libraries on.
const NATIVE_LIBS_NOTE: &str = "native-static-libs:";

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    let rustc = env::var_os("RUSTC").expect("set by cargo");
    let target = env::var("TARGET").expect("set by cargo");

    let source = manifest_dir.join("../baml_native_rt/src/lib.rs");
    let archive = out_dir.join("libbaml_native_rt.a");
    println!("cargo:rerun-if-changed={}", source.display());

    let output = Command::new(rustc)
        .args(["--edition", "2024"])
        .args(["--crate-type", "staticlib"])
        .args(["--crate-name", "baml_native_rt"])
        .args(["--target", &target])
        .args(["-C", "opt-level=3"])
        .args(["-C", "panic=abort"])
        .args(["-C", "codegen-units=1"])
        .args(["--print", "native-static-libs"])
        .arg("-o")
        .arg(&archive)
        .arg(&source)
        .output()
        .expect("rustc runs");
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "compiling the native runtime failed:\n{diagnostics}"
    );
    let native_libs = diagnostics
        .lines()
        .find_map(|line| line.split_once(NATIVE_LIBS_NOTE))
        .map(|(_, libs)| libs.trim())
        .unwrap_or_default();

    println!(
        "cargo:rustc-env=BAML_NATIVE_RT_ARCHIVE={}",
        archive.display()
    );
    println!("cargo:rustc-env=BAML_NATIVE_RT_LINK_LIBS={native_libs}");
}
