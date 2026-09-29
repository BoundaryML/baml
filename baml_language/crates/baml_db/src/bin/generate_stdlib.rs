//! Standalone producer of the stdlib artifact embedded by the runtime.

use std::{env, error::Error, path::Path};

const USAGE: &str = "Usage: baml-generate-stdlib --out-dir <directory>
Writes stdlib_prefix.borsh for bex_project using this compiler build.";

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
    let [flag, out_dir] = args.as_slice() else {
        return Err(USAGE.into());
    };
    if flag != "--out-dir" {
        return Err(USAGE.into());
    }
    baml_db::stdlib_prefix::runtime::generate(Path::new(out_dir))?;
    Ok(())
}
