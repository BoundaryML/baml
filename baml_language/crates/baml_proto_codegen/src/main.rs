//! Explicit maintenance command; ordinary builds never call this executable.

use std::{env, error::Error, path::PathBuf};

const USAGE: &str =
    "Usage: baml-generate-sdk-protos [--proto-dir DIR] [--rust-out DIR] [--python-out DIR]
Defaults to the bridge schemas and committed Rust/Python SDK directories in this checkout.
PROTOC overrides the bundled compiler; without baml-defaults, PROTOC is required.";

#[allow(clippy::print_stdout, reason = "CLI help uses stdout.")]
fn main() -> Result<(), Box<dyn Error>> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut proto_dir = workspace.join("crates/bridge_ctypes/types");
    let mut rust_out = workspace.join("sdks/rust/bridge_rust/src/wire");
    let mut python_out = workspace.join("sdks/python/src");
    let mut args = env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            return Ok(());
        }
        let path = args.next().filter(|value| !value.is_empty()).ok_or(USAGE)?;
        match flag.to_str() {
            Some("--proto-dir") => proto_dir = path.into(),
            Some("--rust-out") => rust_out = path.into(),
            Some("--python-out") => python_out = path.into(),
            _ => return Err(USAGE.into()),
        }
    }
    baml_proto_codegen::generate_sdks(&proto_dir, &rust_out, &python_out)?;
    Ok(())
}
