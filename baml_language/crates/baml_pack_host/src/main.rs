// Runtime host for binaries produced by `baml pack`.
//
// Startup:
//   1. Extract the versioned `PackEnvelope` from the OS-native embedded section.
//   2. Decide single-target vs multi-subcommand dispatch from
//      `envelope.mode`.
//   3. Initialize the BAML engine with the embedded program and the
//      argv shape the chosen mode expects.
//   4. Parse argv through the runtime clap parser
//      ([`baml_exec::parse_target_argv`] or
//      [`baml_exec::parse_multi_target_argv`]) — this gives `--help` /
//      unknown-flag / missing-required handling for free, with the same
//      brand-purple styling as `baml run`'s top-level clap.
//   5. Dispatch to the resolved target via `baml_exec::dispatch_target`
//      and write the return value in the baked-in output format.
//
// Exit codes: 0 on success, non-zero on error. To set a non-zero exit
// code from BAML, the program calls `baml.sys.exit(code)`.

// `process::exit` is intentional — the BAML target may call
// `baml.sys.exit(code)` to short-circuit the engine with a specific
// exit code, which we honour by terminating the process directly.
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use std::{collections::HashMap, process::ExitCode, sync::Arc};

use baml_exec::{
    DispatchResult, PACK_SECTION_NAME, PackEnvelope, PackMode, clamp_exit_code, dispatch_target,
    load_json_source, parse_multi_target_argv, parse_target_argv, print_error,
};
use bex_engine::{BexEngine, UserFunctionInfo};
use sys_native::SysOpsExt;

fn extract_envelope() -> Result<PackEnvelope, String> {
    let section = libsui::find_section(PACK_SECTION_NAME)
        .map_err(|e| format!("Failed to read embedded section: {e}"))?
        .ok_or("No embedded BAML package found. This binary must be built with `baml pack`.")?;

    baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, section)
        .map_err(|e| format!("Failed to deserialize pack envelope: {e}"))
}

/// Build `baml.argv` per BEP-027 §"baml.argv in packaged binaries".
///
/// `argv[1]` is the target identifier for single-target packs; for
/// multi-subcommand packs the host overrides it after parsing the
/// subcommand off the command line (see `main`).
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
fn target_is_typed(info: &UserFunctionInfo) -> bool {
    !info.param_names.is_empty()
}

fn main() -> ExitCode {
    let envelope = match extract_envelope() {
        Ok(e) => e,
        Err(e) => {
            print_error(e);
            return ExitCode::FAILURE;
        }
    };

    match envelope.mode {
        PackMode::Single => run_single(envelope),
        PackMode::Subcommand => run_subcommand(envelope),
    }
}

/// Single-target dispatch: the binary acts like a one-shot CLI; flags on
/// the binary bind directly to the target's parameters.
fn run_single(envelope: PackEnvelope) -> ExitCode {
    debug_assert_eq!(
        envelope.targets.len(),
        1,
        "single mode = exactly one target"
    );
    let target = &envelope.targets[0];

    let argv = build_argv(&target.subcommand_name);

    let engine = match BexEngine::new_with_runtime_compiler(
        envelope.program,
        Arc::new(sys_native::SysOps::native()),
        argv.clone(),
        bex_project::runtime_compiler(),
    ) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            print_error(format_args!("failed to initialize engine: {e}"));
            return ExitCode::FAILURE;
        }
    };

    let Some(func_info) = engine.find_user_function(&target.qualified_name) else {
        print_error(format_args!(
            "function `{}` not found",
            target.qualified_name
        ));
        return ExitCode::FAILURE;
    };

    let raw_cli_tokens: &[String] = if argv.len() > 2 { &argv[2..] } else { &[] };
    let parsed = if target_is_typed(&func_info) {
        let bin_name = std::env::args()
            .next()
            .unwrap_or_else(|| "./binary".to_string());
        match parse_target_argv(
            &bin_name,
            &target.qualified_name,
            &func_info,
            raw_cli_tokens,
        ) {
            Ok(p) => p,
            Err(err) => {
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

    finalize_dispatch(
        &engine,
        &target.qualified_name,
        parsed,
        envelope.output_format,
    )
}

/// Multi-subcommand dispatch: the binary acts like a multi-tool, with
/// one subcommand per packed function.
fn run_subcommand(envelope: PackEnvelope) -> ExitCode {
    // `argv[1]` is the user's subcommand token after parsing; rebuild
    // os-level argv first so we can pass the trailing tokens to clap.
    let mut os_args = std::env::args();
    let exe = os_args
        .next()
        .unwrap_or_else(|| "baml-pack-host".to_string());
    let trailing: Vec<String> = os_args.collect();

    // Engine init needs argv at construction time. Use a placeholder for
    // `argv[1]` and patch it once the subcommand is resolved, while the
    // engine is still uniquely owned (pre-Arc).
    let mut bootstrap_argv = Vec::with_capacity(2 + trailing.len());
    bootstrap_argv.push(exe.clone());
    bootstrap_argv.push(String::new());
    bootstrap_argv.extend(trailing.iter().cloned());

    let mut engine = match BexEngine::new_with_runtime_compiler(
        envelope.program,
        Arc::new(sys_native::SysOps::native()),
        bootstrap_argv,
        bex_project::runtime_compiler(),
    ) {
        Ok(e) => e,
        Err(e) => {
            print_error(format_args!("failed to initialize engine: {e}"));
            return ExitCode::FAILURE;
        }
    };

    // Per-target lookups for the clap subcommand builder. Built once,
    // shared with `parse_multi_target_argv`.
    let mut lookups: HashMap<String, UserFunctionInfo> = HashMap::new();
    for entry in &envelope.targets {
        match engine.find_user_function(&entry.qualified_name) {
            Some(info) => {
                lookups.insert(entry.qualified_name.clone(), info);
            }
            None => {
                print_error(format_args!(
                    "function `{}` not found in packed program",
                    entry.qualified_name
                ));
                return ExitCode::FAILURE;
            }
        }
    }

    let (chosen, parsed) =
        match parse_multi_target_argv(&exe, &envelope.targets, &lookups, &trailing) {
            Ok(v) => v,
            Err(err) => {
                use baml_exec::clap_reexport::ErrorKind;
                let kind = err.kind();
                let _ = err.print();
                // `DisplayHelp` is the "user asked for help" exit-0 path;
                // `DisplayHelpOnMissingArgumentOrSubcommand` is clap auto-
                // showing help because the invocation was invalid, which
                // must exit non-zero so scripted callers can detect it.
                return match kind {
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                    _ => ExitCode::FAILURE,
                };
            }
        };

    // Patch `argv[1]` to the resolved subcommand identifier so
    // `baml.sys.argv()` matches what the user typed (BEP-027 §"`baml.argv`
    // in packaged binaries").
    let chosen_entry = envelope
        .targets
        .iter()
        .find(|t| t.qualified_name == chosen)
        .expect("parse_multi_target_argv returned an unregistered target");
    let mut patched = engine.argv().to_vec();
    if patched.len() >= 2 {
        chosen_entry.subcommand_name.clone_into(&mut patched[1]);
        engine.set_argv(patched);
    }

    let engine = Arc::new(engine);
    finalize_dispatch(&engine, &chosen, parsed, envelope.output_format)
}

fn finalize_dispatch(
    engine: &Arc<BexEngine>,
    target_name: &str,
    parsed: baml_exec::ParsedTargetArgs,
    output_format: baml_exec::OutputFormat,
) -> ExitCode {
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
        Arc::clone(engine),
        target_name,
        parsed.cli_values,
        json_args,
        output_format,
    ));
    rt.block_on(engine.shutdown());
    let mut unhandled_spawn_failed = false;
    for report in engine.take_unhandled_spawn_errors() {
        if report.cancelled {
            let error = report.into_engine_error();
            eprintln!("Warning: cancelled spawned task failed: {error}");
        } else {
            let error = report.into_engine_error();
            print_error(format_args!("unhandled spawned task failed: {error}"));
            unhandled_spawn_failed = true;
        }
    }

    // Drain the direct profiling consumer before exit (no-op when profiling
    // is off). `baml.sys.exit()` paths bypass this explicit host flush.
    bex_events::prof::flush_and_join(std::time::Duration::from_secs(10));

    match result {
        Ok(DispatchResult::Ok) if !unhandled_spawn_failed => ExitCode::SUCCESS,
        Ok(DispatchResult::Ok) => ExitCode::FAILURE,
        Ok(DispatchResult::TargetError) => ExitCode::FAILURE,
        Ok(DispatchResult::Exit(code)) => std::process::exit(clamp_exit_code(code)),
        Err(e) => {
            print_error(e);
            ExitCode::FAILURE
        }
    }
}
