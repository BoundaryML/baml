//! Standalone entry point for build systems that do not run Cargo build scripts.

use std::{env, error::Error, path::Path};

use baml_builtins2_codegen::{BuiltinCrate, generate};

const USAGE: &str = "Usage: baml-generate-builtins <crate> --out-dir <directory>
Crates: bex_vm, bex_vm_types, sys_ops, sys_types
Use a separate output directory for each crate.";

#[allow(
    clippy::print_stdout,
    reason = "Command-line help is written to stdout."
)]
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let [consumer, flag, out_dir] = args.as_slice() else {
        return Err(USAGE.into());
    };
    if flag != "--out-dir" {
        return Err(USAGE.into());
    }
    let consumer = match consumer.to_str() {
        Some("bex_vm") => BuiltinCrate::BexVm,
        Some("bex_vm_types") => BuiltinCrate::BexVmTypes,
        Some("sys_ops") => BuiltinCrate::SysOps,
        Some("sys_types") => BuiltinCrate::SysTypes,
        _ => return Err(format!("Unknown crate {consumer:?}.\n{USAGE}").into()),
    };
    generate(consumer, Path::new(out_dir))
}
