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

    warn_if_direct_invocation();

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
    // Streams spec §7.5: flush the profiler's pending stream segments on
    // every exit path (the durability window is otherwise publish_interval).
    bex_events::prof::flush_and_join(std::time::Duration::from_secs(5));
    std::process::exit(exit_code.into());
}

fn warn_if_direct_invocation() {
    if env_flag("BAML_WRAPPER_EXEC") || env_flag("BAML_CLI_ALLOW_DIRECT") {
        return;
    }
    let _ = writeln!(
        std::io::stderr(),
        "warning: using the internal BAML toolchain binary directly is not recommended. Use `baml` instead."
    );
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true"))
        .unwrap_or(false)
}
