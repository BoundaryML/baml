// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

use anyhow::Result;
// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

fn main() -> Result<()> {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    let argv: Vec<String> = std::env::args().collect();

    // `run_cli` returns an `ExitCode` variant describing how the verb
    // finished. The real process exit is deferred here so `run_cli` and
    // its callees stay testable (no inline `std::process::exit`).
    let exit_code = baml_cli::run_cli(argv)?;
    match exit_code {
        baml_cli::ExitCode::Success => Ok(()),
        other => std::process::exit(other.into()),
    }
}
