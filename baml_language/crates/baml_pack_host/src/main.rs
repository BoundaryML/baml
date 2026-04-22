// Runtime host for binaries produced by `baml pack`.
//
// Startup:
//   1. Extract `PackEnvelope` (bitcode) from the OS-native embedded section.
//   2. Build `baml.argv` per BEP-027 §"baml.argv in packaged binaries":
//        argv[0] = path to this binary
//        argv[1] = target identifier baked in at pack time
//        argv[2+] = every token on the command line after the binary name
//   3. Initialize the BAML engine with the embedded program and argv.
//   4. If the baked-in target is typed, short-circuit on `--help` and
//      print auto-derived help.
//   5. Otherwise dispatch to the target via `baml_exec::dispatch_target`,
//      which parses argv[2..] as auto-CLI flags against the target
//      signature and writes the return value to stdout in the baked-in
//      output format.
//
// Exit codes: 0 on success, non-zero on error. The return value is NOT
// overloaded as an exit code (BEP-027 §"Exit codes"); to set a non-zero
// exit code, the program calls `baml.sys.exit(code)`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{process::ExitCode, sync::Arc};

use baml_exec::{
    DispatchResult, PackEnvelope, clamp_exit_code, dispatch_target, print_target_help,
};
use bex_engine::BexEngine;
use sys_native::SysOpsExt;

const SECTION_NAME: &str = "baml_pack";

fn extract_envelope() -> Result<PackEnvelope, String> {
    let section = libsui::find_section(SECTION_NAME)
        .map_err(|e| format!("Failed to read embedded section: {e}"))?
        .ok_or("No embedded BAML package found. This binary must be built with `baml pack`.")?;

    bitcode::deserialize(section).map_err(|e| format!("Failed to deserialize pack envelope: {e}"))
}

/// Build `baml.argv` per BEP-027 §"baml.argv in packaged binaries".
fn build_argv(target_identifier: &str) -> Vec<String> {
    let mut os_args = std::env::args();
    let exe = os_args.next().unwrap_or_else(|| "baml-host".to_string());
    let mut argv = Vec::with_capacity(2 + os_args.len());
    argv.push(exe);
    argv.push(target_identifier.to_string());
    argv.extend(os_args);
    argv
}

/// Whether the target is typed — i.e. has one or more parameters whose
/// types drive the auto-CLI. Parameterless targets own their full argv
/// and receive no injected `--help`.
fn target_is_typed(engine: &BexEngine, target_name: &str) -> bool {
    engine
        .function_params(target_name)
        .map(|params| !params.is_empty())
        .unwrap_or(false)
}

fn handle_help(engine: &BexEngine, target_name: &str) -> bool {
    let info = engine.user_functions().into_iter().find(|f| {
        f.qualified_name == target_name
            || f.display_name == target_name.strip_prefix("user.").unwrap_or(target_name)
    });
    let Some(info) = info else { return false };

    let display = target_name.strip_prefix("user.").unwrap_or(target_name);
    // Reconstruct the invocation prefix from argv[0] so the help output
    // matches how the user actually invoked the binary (e.g. `./summarize`).
    let exe = std::env::args()
        .next()
        .unwrap_or_else(|| "./binary".to_string());
    let prefix = format!("{exe} ");
    let _ = display; // display currently unused; retained for future log lines
    print_target_help(target_name, &info, &prefix);
    true
}

fn main() -> ExitCode {
    let envelope = match extract_envelope() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let argv = build_argv(&envelope.target_identifier);

    let engine = match BexEngine::new(
        envelope.program,
        Arc::new(sys_native::SysOps::native()),
        None,
        argv.clone(),
    ) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            eprintln!("error: failed to initialize engine: {e}");
            return ExitCode::FAILURE;
        }
    };

    let cli_tokens: &[String] = if argv.len() > 2 { &argv[2..] } else { &[] };

    // `--help` is reserved on typed targets only; parameterless `main()`
    // owns its full argv and can handle `--help` itself.
    if target_is_typed(&engine, &envelope.target_name)
        && cli_tokens.iter().any(|t| t == "--help" || t == "-h")
        && handle_help(&engine, &envelope.target_name)
    {
        return ExitCode::SUCCESS;
    }

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to create runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = rt.block_on(dispatch_target(
        engine,
        &envelope.target_name,
        cli_tokens,
        None,
        envelope.output_format,
    ));

    match result {
        Ok(DispatchResult::Ok) => ExitCode::SUCCESS,
        Ok(DispatchResult::TargetError) => ExitCode::FAILURE,
        // `baml.sys.exit(code)`: narrow to `i32` (the `std::process::exit`
        // contract) and terminate. Further OS-specific narrowing — the
        // low 8 bits on Unix — is the shell's problem.
        Ok(DispatchResult::Exit(code)) => std::process::exit(clamp_exit_code(code)),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
