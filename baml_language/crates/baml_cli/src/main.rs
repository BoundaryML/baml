// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

use anyhow::Result;
// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

fn main() -> Result<()> {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    let argv: Vec<String> = std::env::args().collect();

    let exit_code = baml_cli::run_cli(argv)?;
    std::process::exit(exit_code.into());
}
