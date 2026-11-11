#![warn(clippy::disallowed_methods)]
// TODO: This file has been simplified to remove baml_runtime/baml_log dependencies.

// TODO: baml_runtime is disabled for now
// use baml_runtime::RuntimeCliDefaults;

use std::io::Write as _;

/// The compiler workload is dominated by small short-lived allocations
/// (`Ty` trees, `Vec`s, `SmolStr`s): the system allocator measured ~35% of
/// remaining single-threaded CPU in the cold-compile audit. Installing
/// mimalloc as the global allocator substantially cuts cold `baml check` /
/// `baml build` wall time. This affects allocation only, not any rendered
/// bytes, so it is safe with respect to `DEV_BAML_BUILD_CACHE_VERIFY`.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // TODO: baml_log is disabled for now
    // baml_log::init()?;

    // `check` lowers and type-checks files on rayon's workers, and the
    // compiler recurses once per nesting level (the parser bounds nesting;
    // lowering and type checking follow the same shape). The default 2 MiB
    // worker stack is tight for that in a debug build: reserve 16 MiB per
    // worker, virtual and committed lazily. `RUST_MIN_STACK` in
    // `.cargo/config.toml` gives tests the same figure.
    rayon::ThreadPoolBuilder::new()
        .stack_size(16 << 20)
        .build_global()
        .unwrap_or_else(|error| unreachable!("the global pool is built once, first: {error}"));

    if let Err(error) = warn_if_direct_invocation() {
        let _ = writeln!(std::io::stderr(), "error: {error}");
        std::process::exit(baml_cli::ExitCode::Other.into());
    }

    let argv: Vec<String> = std::env::args().collect();

    // Route every top-level error through the graphical printer
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

fn warn_if_direct_invocation() -> Result<(), baml_env::EnvError> {
    if baml_env::bool_var("BAML_CLI_ALLOW_DIRECT")? == Some(true) {
        return Ok(());
    }
    let _ = writeln!(
        std::io::stderr(),
        "warning: using the internal BAML toolchain binary directly is not recommended. Use `baml` instead."
    );
    Ok(())
}
