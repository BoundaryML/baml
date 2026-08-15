use std::{env, fs, path::PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir.parent().unwrap().parent().unwrap();
    let roots = [workspace.join("Cargo.toml"), workspace.join("Cargo.lock")];
    let mut files = roots.to_vec();

    for entry in WalkDir::new(workspace.join("crates"))
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "target")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs" | "baml" | "toml")
        ) {
            files.push(path.to_path_buf());
        }
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path.strip_prefix(workspace).unwrap_or(&path);
        let bytes = fs::read(&path).unwrap();
        hasher.update((relative.as_os_str().len() as u64).to_le_bytes());
        hasher.update(relative.as_os_str().as_encoded_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }

    let digest: [u8; 32] = hasher.finalize().into();
    let values = digest
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("build_id.rs");
    fs::write(
        output,
        format!("pub const COMPILER_BUILD_ID: [u8; 32] = [{values}];\n"),
    )
    .unwrap();
}
