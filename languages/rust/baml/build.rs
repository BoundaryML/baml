fn main() {
    println!("cargo:rerun-if-changed=../../../engine/language_client_cffi/types/");

    let proto_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../engine/language_client_cffi/types");

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

    // Link the CFFI static library
    // The vendored crate provides link-search but we need to also link the library here
    // since the vendored crate is metadata-only (no actual code)
    link_cffi_library();
}

fn link_cffi_library() {
    let target = std::env::var("TARGET").unwrap();
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // Map target triple to vendored crate directory
    let ffi_crate = match target.as_str() {
        "aarch64-apple-darwin" => "baml-ffi-aarch64-apple-darwin",
        "x86_64-apple-darwin" => "baml-ffi-x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu" => "baml-ffi-x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu" => "baml-ffi-aarch64-unknown-linux-gnu",
        "x86_64-pc-windows-msvc" => "baml-ffi-x86_64-pc-windows-msvc",
        _ => {
            println!("cargo:warning=Unsupported target: {target}. FFI calls will fail.");
            return;
        }
    };

    let lib_dir = manifest_dir.join("..").join(ffi_crate).join("lib");

    // Determine library name based on platform
    let (lib_name, lib_file) = if target.contains("windows") {
        ("baml_cffi", "baml_cffi.lib")
    } else {
        ("baml_cffi", "libbaml_cffi.a")
    };

    let lib_path = lib_dir.join(lib_file);
    if lib_path.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static={lib_name}");

        // Link system libraries required by the CFFI library
        link_system_libraries(&target);
    } else {
        println!(
            "cargo:warning={lib_file} not found at {}. FFI calls will fail at link time.",
            lib_path.display()
        );
    }
}

fn link_system_libraries(target: &str) {
    if target.contains("apple") {
        // macOS system frameworks required by the CFFI library
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=CoreServices");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
    } else if target.contains("linux") {
        // Linux system libraries (if needed)
        // Usually these are provided by the system
    } else if target.contains("windows") {
        // Windows system libraries (if needed)
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=userenv");
        println!("cargo:rustc-link-lib=bcrypt");
    }
}
