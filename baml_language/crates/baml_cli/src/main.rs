// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

fn main() {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    let argv: Vec<String> = std::env::args().collect();

    // Route every top-level error through the ariadne-style printer
    // (bold-red "Error:" header + cause chain) so plain bails look the
    // same as compiler diagnostics. Returning `Result<()>` from `main`
    // would defer to anyhow's Debug printer instead, which renders the
    // header uncolored.
    let exit_code = match baml_cli::run_cli(argv) {
        Ok(code) => code,
        Err(e) => {
            baml_cli::reporter::print_anyhow_error(&e);
            baml_cli::ExitCode::Other
        }
    };
    std::process::exit(exit_code.into());
}
