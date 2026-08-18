//! The C++ protobuf bindings for the CFFI wire schema are GENERATED with
//! the repo's pinned protoc (protoc-bin-vendored, the same one
//! `bridge_ctypes` uses for prost) and checked in under
//! `sdks/cpp/bridge_cpp/pb/`; this test pins them so a schema change breaks
//! CI instead of decoding wrong at runtime. Mirrors `bridge_cffi`'s
//! `header_generation.rs` pattern (drift test + `--ignored` regenerate
//! bless test).
//!
//! The generated code targets the LITE runtime (`option optimize_for =
//! LITE_RUNTIME` in the .proto sources) and must be compiled against the
//! protobuf runtime version the emitted `CMakeLists` pins, which matches
//! this protoc.

use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

const PROTO_FILES: &[&str] = &[
    "baml_bridge/cffi/v1/baml_handle.proto",
    "baml_bridge/cffi/v1/baml_inbound.proto",
    "baml_bridge/cffi/v1/baml_outbound.proto",
    "baml_bridge/cffi/v1/baml_type.proto",
];

fn workspace_root() -> PathBuf {
    // No canonicalize: on Windows it yields a \\?\ extended-length path,
    // which protoc cannot relate its proto files to.
    //
    // BAML_WORKSPACE_ROOT binds the root when set: the compile-time
    // CARGO_MANIFEST_DIR is baked, and for the CI nix unit graph it is a
    // standalone store copy of this crate whose ../../.. holds no sibling
    // crates - protoc then dies "Could not make proto path relative"
    // (proven live, run 32106655757, both pb_generation tests, every
    // gnu/musl probe hit). Unset, byte-identical. Same pattern as
    // BAML_BRIDGE_CFFI_DIR one fix over.
    if let Some(root) = std::env::var_os("BAML_WORKSPACE_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn pb_root() -> PathBuf {
    workspace_root().join("sdks/cpp/bridge_cpp/pb")
}

/// Runs the pinned protoc into a temp dir and returns rel path -> content
/// for every generated file.
fn generate() -> BTreeMap<String, String> {
    let types_root = workspace_root().join("crates/bridge_ctypes/types");
    let out = tempfile::tempdir().expect("temp dir");
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    // Absolute file paths under --proto_path (bridge_ctypes/build.rs does
    // the same): protoc rejects relative args it cannot map on Windows.
    let status = Command::new(protoc)
        .arg(format!("--proto_path={}", types_root.display()))
        .arg(format!("--cpp_out={}", out.path().display()))
        .args(PROTO_FILES.iter().map(|p| types_root.join(p)))
        .status()
        .expect("run protoc");
    assert!(status.success(), "protoc failed");

    let mut files = BTreeMap::new();
    for proto in PROTO_FILES {
        for ext in ["pb.h", "pb.cc"] {
            let rel = proto.replace(".proto", &format!(".{ext}"));
            let content = fs::read_to_string(out.path().join(&rel))
                .unwrap_or_else(|e| panic!("protoc did not produce {rel}: {e}"))
                .replace("\r\n", "\n");
            // protoc emits trailing whitespace on some lines; strip it so
            // the checked-in files satisfy the repo's whitespace hooks and
            // the drift comparison stays byte-exact.
            let content = content
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            files.insert(rel, content);
        }
    }
    files
}

#[test]
fn checked_in_pb_sources_match_proto() {
    for (rel, expected) in generate() {
        let path = pb_root().join(&rel);
        let actual = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .replace("\r\n", "\n");
        assert_eq!(
            actual,
            expected,
            "{} drifted from the CFFI .proto schema (or the pinned protoc \
             changed); run the documented regeneration command",
            path.display()
        );
    }
}

#[test]
fn generated_code_is_lite() {
    for (rel, content) in generate() {
        if rel.ends_with(".pb.h") {
            assert!(
                content.contains("MessageLite"),
                "{rel} is not lite-runtime code; check optimize_for in the \
                 .proto sources"
            );
        }
    }
}

#[test]
#[ignore = "writes the checked-in generated protobuf sources"]
fn regenerate() {
    for (rel, content) in generate() {
        let path = pb_root().join(&rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create pb dir");
        fs::write(&path, content).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}
