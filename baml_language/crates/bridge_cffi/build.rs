fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_outbound.proto");
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_inbound.proto");
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_object.proto");
    println!("cargo:rerun-if-changed=types/baml/cffi/v1/baml_object_methods.proto");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");

    unsafe {
        std::env::set_var(
            "PROTOC",
            protoc_bin_vendored::protoc_bin_path()
                .unwrap()
                .to_str()
                .unwrap(),
        );
    }

    let protos = [
        "types/baml/cffi/v1/baml_outbound.proto",
        "types/baml/cffi/v1/baml_inbound.proto",
        "types/baml/cffi/v1/baml_object.proto",
        "types/baml/cffi/v1/baml_object_methods.proto",
    ];

    prost_build::compile_protos(&protos, &["types"])?;

    Ok(())
}
