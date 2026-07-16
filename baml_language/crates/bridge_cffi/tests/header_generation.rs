use std::{fs, path::PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

#[test]
fn checked_in_header_matches_rust_abi() {
    let path = crate_dir().join("include/baml_cffi.h");
    let actual = fs::read(&path).expect("checked-in public header must exist");
    let expected = generated_bytes();
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
    let api = fs::read_to_string(root.join("src/api.rs")).expect("read API declarations");
    let buffer = fs::read_to_string(root.join("src/buffer.rs")).expect("read buffer declaration");
    let runtime =
        fs::read_to_string(root.join("src/ffi/runtime.rs")).expect("read bridge-info declaration");

    assert!(api.contains("#[repr(C)]\npub struct BamlApiV1"));
    assert!(buffer.contains("#[repr(C)]\npub struct Buffer"));
    assert!(runtime.contains("#[repr(C)]\npub struct BamlBridgeInfoV1"));
}

#[test]
#[ignore = "writes the checked-in public header"]
fn regenerate() {
    let path = crate_dir().join("include/baml_cffi.h");
    fs::create_dir_all(path.parent().unwrap()).expect("create public include directory");
    generate().write_to_file(&path);
}
