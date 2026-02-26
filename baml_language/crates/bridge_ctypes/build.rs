fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_outbound.proto");
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_inbound.proto");
    println!("cargo:rerun-if-changed=build.rs");

    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(
            "PROTOC",
            protoc_bin_vendored::protoc_bin_path()
                .expect("failed to locate vendored protoc binary")
                .to_str()
                .expect("protoc path contains invalid UTF-8"),
        );
    }

    let protos = [
        "types/baml/cffi/v1/baml_outbound.proto",
        "types/baml/cffi/v1/baml_inbound.proto",
    ];

    prost_build::compile_protos(&protos, &["types"])?;

    Ok(())
}
