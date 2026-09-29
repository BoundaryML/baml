//! Shared protobuf compiler selection for build scripts and SDK maintenance.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

/// Resolve the compiler without changing the environment or acquiring tools.
/// An explicit override always wins, including when it is invalid.
pub fn protoc() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("PROTOC") {
        if path.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PROTOC must name a protobuf compiler executable",
            ));
        }
        return Ok(PathBuf::from(path));
    }
    #[cfg(feature = "baml-defaults")]
    {
        protoc_bin_vendored::protoc_bin_path().map_err(io::Error::other)
    }
    #[cfg(not(feature = "baml-defaults"))]
    {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Set PROTOC to a protobuf compiler executable; the bundled compiler is disabled without baml-defaults",
        ))
    }
}

/// Find all schemas recursively, in deterministic order. Read errors are fatal.
pub fn schemas(directory: &Path) -> io::Result<Vec<PathBuf>> {
    fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if fs::metadata(&path)?.is_dir() {
                collect(&path, paths)?;
            } else if path
                .extension()
                .is_some_and(|extension| extension == "proto")
            {
                paths.push(path);
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    collect(directory, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No protobuf schemas in {}", directory.display()),
        ));
    }
    Ok(paths)
}

/// Compile Rust bindings into Cargo's `OUT_DIR` and declare generation inputs.
#[allow(
    clippy::print_stdout,
    reason = "Cargo build-script directives use stdout."
)]
pub fn compile(protos: &[impl AsRef<Path>], includes: &[impl AsRef<Path>]) -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PROTOC");
    println!("cargo:rerun-if-env-changed=PROTOC_INCLUDE");
    println!("cargo:rerun-if-env-changed=PATH");
    for path in protos
        .iter()
        .map(AsRef::as_ref)
        .chain(includes.iter().map(AsRef::as_ref))
    {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let protoc = protoc()?;
    if protoc.is_file() {
        println!("cargo:rerun-if-changed={}", protoc.display());
    }
    if let Some(include) = env::var_os("PROTOC_INCLUDE") {
        println!("cargo:rerun-if-changed={}", Path::new(&include).display());
    }
    prost_build::Config::new()
        .protoc_executable(protoc)
        .compile_protos(protos, includes)
}

/// Regenerate committed SDK sources only when explicitly requested.
pub fn generate_sdks(proto_dir: &Path, rust_out: &Path, python_out: &Path) -> io::Result<()> {
    let proto_dir = proto_dir.canonicalize()?;
    let protos = schemas(&proto_dir)?;
    let protoc = protoc()?;
    fs::create_dir_all(rust_out)?;
    fs::create_dir_all(python_out)?;
    // Preserve the committed Rust output without toolchain-dependent formatting.
    prost_build::Config::new()
        .protoc_executable(&protoc)
        .out_dir(rust_out)
        .format(false)
        .compile_protos(&protos, &[&proto_dir])?;
    let status = std::process::Command::new(&protoc)
        .arg(format!("--proto_path={}", proto_dir.display()))
        .arg(format!("--python_out={}", python_out.display()))
        .arg(format!("--pyi_out={}", python_out.display()))
        .args(&protos)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "{} failed to generate Python bindings: {status}",
            protoc.display()
        )));
    }
    Ok(())
}
