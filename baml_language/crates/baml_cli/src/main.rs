// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

use std::io::Write as _;

/// The compiler workload is dominated by small short-lived allocations
/// (`Ty` trees, `Vec`s, `SmolStr`s): the system allocator measured ~35% of
/// remaining single-threaded CPU in the cold-compile audit. Installing
/// mimalloc as the global allocator substantially cuts cold `baml check` /
/// `baml build` wall time. This affects allocation only, not any rendered
/// bytes, so it is safe with respect to `BAML_CACHE_VERIFY`.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    let argv: Vec<String> = std::env::args().collect();
    if argv.len() == 2 && argv[1] == baml_shell::ROOT_HELP_COMMAND_V1 {
        let mut stdout = std::io::stdout().lock();
        let result = (|| -> anyhow::Result<()> {
            serde_json::to_writer(&mut stdout, &baml_cli::root_help_v1())?;
            writeln!(stdout)?;
            Ok(())
        })();
        if let Err(err) = result {
            baml_cli::reporter::print_error(err);
            std::process::exit(1);
        }
        return;
    }
    warn_if_direct_invocation();

    // Route every top-level error through the graphical printer
    // (bold-red `error:` header + cause chain) so plain bails look the
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

fn warn_if_direct_invocation() {
    if env_flag("BAML_WRAPPER_EXEC") || env_flag("BAML_CLI_ALLOW_DIRECT") {
        return;
    }
    drop(baml_shell::Shell::new().warn(
        "using the internal BAML toolchain binary directly is not recommended. Use `baml` instead.",
    ));
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}
