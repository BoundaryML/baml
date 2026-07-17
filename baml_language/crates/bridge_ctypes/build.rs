use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_dir = manifest_dir.join("types");

    // Discover all .proto files. Sort for a deterministic order: `read_dir`
    // (inside `walkdir`) yields entries in filesystem-dependent order, and
    // prost emits message blocks in proto-argument order — so an unsorted list
    // makes the generated file's block order vary by platform (macOS locally
    // vs Linux in CI), dirtying the committed vendored copy in CI. Sorting by
    // path keeps the generated output byte-identical everywhere.
    let mut protos: Vec<PathBuf> = walkdir(&proto_dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();
    protos.sort();

    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    let protoc =
        protoc_bin_vendored::protoc_bin_path().expect("failed to locate vendored protoc binary");

    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(
            "PROTOC",
            protoc.to_str().expect("protoc path contains invalid UTF-8"),
        );
    }

    // Generate Rust (prost).
    let proto_strs: Vec<&str> = protos
        .iter()
        .map(|p| p.strip_prefix(&manifest_dir).unwrap())
        .map(|p| p.to_str().unwrap())
        .collect();
    prost_build::compile_protos(&proto_strs, &["types"])?;

    // Vendor the same prost output into baml_bridge (the published Rust
    // SDK runtime): it ships committed generated code so consumers need
    // neither protoc nor this crate's engine-coupled codecs. The
    // proto-sync CI job keeps the committed copy honest, like the
    // Python/Go/Node outputs.
    let rust_vendor_out = manifest_dir.join("../../sdks/rust/bridge_rust/src/wire");
    std::fs::create_dir_all(&rust_vendor_out)?;
    let mut vendor_config = prost_build::Config::new();
    vendor_config.out_dir(&rust_vendor_out);
    // Do NOT run rustfmt on the committed vendored copy. This file is written
    // back into the source tree on every build, so its content must be
    // deterministic — but rustfmt's output for a given input varies by
    // toolchain version, which would dirty the tree in CI vs locally. Raw
    // prost output is stable (pinned via Cargo.lock). `cargo fmt` never
    // reformats it either: `src/wire/mod.rs` pulls it in with `include!`, and
    // rustfmt only walks `mod` declarations, not `include!` targets.
    vendor_config.format(false);
    vendor_config.compile_protos(&proto_strs, &["types"])?;

    // Generate Python pb2 + pyi.
    let python_out = manifest_dir.join("../../sdks/python/src");
    let mut cmd = std::process::Command::new(&protoc);
    cmd.arg(format!("--proto_path={}", proto_dir.display()));
    cmd.arg(format!("--python_out={}", python_out.display()));
    cmd.arg(format!("--pyi_out={}", python_out.display()));
    for proto in &protos {
        cmd.arg(proto);
    }
    let status = cmd.status().expect("failed to run protoc");
    assert!(status.success(), "protoc (python) failed with {status}");

    Ok(())
}

fn walkdir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() {
                out.extend(walkdir(&path));
            }
        }
    }
    out
}
