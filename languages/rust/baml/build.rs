fn main() {
    println!("cargo:rerun-if-changed=types/");

    let proto_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("types");

    // Generate proto files into OUT_DIR (standard prost pattern)
    prost_build::Config::new()
        .compile_protos(
            &[
                proto_root.join("baml/cffi/v1/baml_inbound.proto"),
                proto_root.join("baml/cffi/v1/baml_outbound.proto"),
                proto_root.join("baml/cffi/v1/baml_object.proto"),
                proto_root.join("baml/cffi/v1/baml_object_methods.proto"),
            ],
            &[&proto_root],
        )
        .expect("Failed to compile protos");

    // The baml-sys crate handles dynamic library loading at runtime.
    // #[cfg(feature = "auto-download")]
    // baml_sys::ensure_library().expect("Failed to find/download BAML
    // library");
}
