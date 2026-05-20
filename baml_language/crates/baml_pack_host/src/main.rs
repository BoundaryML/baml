// Runtime host for binaries produced by `baml pack`.
//
// Startup:
//   1. Extract `PackEnvelope` (bitcode) from the OS-native embedded section.
//   2. Build `baml.argv` per BEP-027 §"baml.argv in packaged binaries":
//        argv[0] = path to this binary
//        argv[1] = target identifier baked in at pack time
//        argv[2+] = every token on the command line after the binary name
//   3. Initialize the BAML engine with the embedded program and argv.
//   4. Parse argv[2..] through the runtime clap parser
//      ([`baml_exec::parse_target_argv`]) — this gives `--help` /
//      unknown-flag / missing-required handling for free, with the same
//      brand-purple styling as `baml run`'s top-level clap.
//   5. Dispatch to the target via `baml_exec::dispatch_target` and write
//      the return value in the baked-in output format.
//
// Exit codes: 0 on success, non-zero on error. To set a non-zero exit
// code from BAML, the program calls `baml.sys.exit(code)`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{process::ExitCode, sync::Arc};

use baml_exec::{
    DispatchResult, PackEnvelope, clamp_exit_code, dispatch_target, load_json_source,
    parse_target_argv, print_error,
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
    let exe = os_args
        .next()
        .unwrap_or_else(|| "baml-pack-host".to_string());
    let mut argv = Vec::with_capacity(2 + os_args.len());
    argv.push(exe);
    argv.push(target_identifier.to_string());
    argv.extend(os_args);
    argv
}

/// Whether the target is typed — i.e. has one or more parameters whose
/// types drive the auto-CLI. Parameterless targets own their full argv
/// and bypass the clap layer entirely.
fn target_is_typed(engine: &BexEngine, target_name: &str) -> bool {
    engine
        .function_params(target_name)
        .map(|params| !params.is_empty())
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let envelope = match extract_envelope() {
        Ok(e) => e,
        Err(e) => {
            print_error(e);
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
            print_error(format_args!("failed to initialize engine: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let raw_cli_tokens: &[String] = if argv.len() > 2 { &argv[2..] } else { &[] };

    // Parameterless targets own their full argv — including `--help`,
    // which they're free to interpret however they like. Skip the clap
    // layer for them. Typed targets route through clap so help/parse
    // errors are uniformly rendered.
    let parsed = if target_is_typed(&engine, &envelope.target_name) {
        let Some(func_info) = engine.find_user_function(&envelope.target_name) else {
            print_error(format_args!(
                "function `{}` not found",
                envelope.target_name
            ));
            return ExitCode::FAILURE;
        };
        let bin_name = std::env::args()
            .next()
            .unwrap_or_else(|| "./binary".to_string());
        match parse_target_argv(&bin_name, &envelope.target_name, &func_info, raw_cli_tokens) {
            Ok(parsed) => parsed,
            Err(err) => {
                // Clap classifies help/version requests as `Err` with a
                // specific kind; route those to stdout + success so
                // `./packed-bin --help` exits cleanly. Other kinds
                // (missing required, unknown arg, etc.) print to stderr
                // and fail.
                use baml_exec::clap_reexport::ErrorKind;
                let kind = err.kind();
                let _ = err.print();
                return match kind {
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                    _ => ExitCode::FAILURE,
                };
            }
        }
    } else {
        baml_exec::ParsedTargetArgs::default()
    };

    let json_args = match parsed.json_source {
        Some(source) => match load_json_source(&source) {
            Ok(v) => Some(v),
            Err(e) => {
                print_error(e);
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            print_error(format_args!("failed to create runtime: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let result = rt.block_on(dispatch_target(
        engine,
        &envelope.target_name,
        parsed.cli_values,
        json_args,
        envelope.output_format,
    ));

    match result {
        Ok(DispatchResult::Ok) => ExitCode::SUCCESS,
        Ok(DispatchResult::TargetError) => ExitCode::FAILURE,
        // `baml.sys.exit(code)`: narrow to `i32` and terminate. Further
        // OS-specific narrowing — the low 8 bits on Unix — is the shell's
        // problem.
        Ok(DispatchResult::Exit(code)) => std::process::exit(clamp_exit_code(code)),
        Err(e) => {
            print_error(e);
            ExitCode::FAILURE
        }
    }
}
