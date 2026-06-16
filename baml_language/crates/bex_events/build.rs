//! Generates the `.bamlprof` protobuf types (`prof::pb`) from
//! `src/prof/proto/bamlprof.proto` into `OUT_DIR`. Same vendored-protoc
//! pattern as `bridge_ctypes/build.rs`; Rust-only output, nothing committed.

#![allow(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "src/prof/proto/bamlprof.proto";
    println!("cargo:rerun-if-changed={proto}");

    // Build scripts are single-threaded; set_var is sound here.
    unsafe { std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?) };
    prost_build::compile_protos(&[proto], &["src/prof/proto"])?;
    Ok(())
}
