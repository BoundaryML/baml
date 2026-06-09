use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_dir = manifest_dir.join("types");

    // Discover all .proto files.
    let protos: Vec<PathBuf> = walkdir(&proto_dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

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
