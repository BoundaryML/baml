//! Generates the `.bamlprof`, `.bamlvalue`, and `.bamldict` protobuf types
//! into `OUT_DIR`. Same vendored-protoc pattern as `bridge_ctypes/build.rs`;
//! Rust-only output, nothing committed.

#![allow(unsafe_code)]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prof_proto = "src/prof/proto/bamlprof.proto";
    let value_proto = "src/value/proto/bamlvalue.proto";
    let dictionary_proto = "src/revision_dictionary/proto/bamldict.proto";
    println!("cargo:rerun-if-changed={prof_proto}");
    println!("cargo:rerun-if-changed={value_proto}");
    println!("cargo:rerun-if-changed={dictionary_proto}");

    // Build scripts are single-threaded; set_var is sound here.
    unsafe { std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?) };
    prost_build::compile_protos(
        &[prof_proto, value_proto, dictionary_proto],
        &[
            "src/prof/proto",
            "src/value/proto",
            "src/revision_dictionary/proto",
        ],
    )?;
    Ok(())
}
