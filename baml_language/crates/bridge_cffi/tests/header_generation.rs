#![cfg(not(target_os = "windows"))]

use std::{fs, path::PathBuf};

/// This crate's source dir, which `generate()` hands to cbindgen (which in
/// turn runs `cargo metadata` on it). The compile-time `CARGO_MANIFEST_DIR`
/// is baked into the test binary, and for the CI nix unit graph that is a
/// standalone store copy of the crate with no workspace root - cargo
/// metadata dies there with a workspace-root resolution error (proven
/// live, run 32090446461). `BAML_BRIDGE_CFFI_DIR` binds the real
/// checkout's crate dir when set; unset, behavior is byte-identical. Same
/// pattern as `BAML_SURFACE_SNAPSHOT_DIR` and `BAML_PARAM_SCHEMA_GOLDEN`.
fn crate_dir() -> PathBuf {
    std::env::var_os("BAML_BRIDGE_CFFI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn generate() -> cbindgen::Bindings {
    let root = crate_dir();
    let config = cbindgen::Config::from_file(root.join("cbindgen.toml"))
        .expect("cbindgen.toml must be valid");
    cbindgen::Builder::new()
        .with_crate(&root)
        .with_config(config)
        .generate()
        .expect("public C header generation must succeed")
}

fn generated_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    generate().write(&mut bytes);
    bytes
}

fn normalize_line_endings(text: String) -> String {
    text.replace("\r\n", "\n")
}

fn read_normalized(path: PathBuf) -> String {
    normalize_line_endings(fs::read_to_string(path).expect("source file must be UTF-8"))
}

#[test]
fn checked_in_header_matches_rust_abi() {
    let path = crate_dir().join("include/baml_cffi.h");
    let actual = read_normalized(path.clone());
    let expected = normalize_line_endings(
        String::from_utf8(generated_bytes()).expect("generated header must be UTF-8"),
    );
    assert_eq!(
        actual,
        expected,
        "{} drifted from the Rust ABI; run the documented regeneration command",
        path.display()
    );
}

#[test]
fn generation_is_byte_for_byte_deterministic() {
    assert_eq!(generated_bytes(), generated_bytes());
}

#[test]
fn public_structs_explicitly_use_c_layout() {
    let root = crate_dir();
    let api = read_normalized(root.join("src/api.rs"));
    let buffer = read_normalized(root.join("src/buffer.rs"));
    let runtime = read_normalized(root.join("src/ffi/runtime.rs"));

    assert!(api.contains("#[repr(C)]\npub struct BamlApiV1"));
    assert!(buffer.contains("#[repr(C)]\npub struct Buffer"));
    assert!(runtime.contains("#[repr(C)]\npub struct BamlBridgeInfoV1"));
}

#[test]
#[ignore = "writes the checked-in public header"]
fn regenerate() {
    let path = crate_dir().join("include/baml_cffi.h");
    fs::create_dir_all(path.parent().unwrap()).expect("create public include directory");
    fs::write(path, generated_bytes()).expect("write checked-in public header");
}
