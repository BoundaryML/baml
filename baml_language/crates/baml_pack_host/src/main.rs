// Runtime host for binaries produced by `baml pack`.
//
// Startup:
//   1. Extract the versioned `PackEnvelope` from the OS-native embedded section.
//   2. Decide single-target vs multi-subcommand dispatch from
//      `envelope.mode`.
//   3. Parse argv against the program's signatures
//      ([`baml_exec::parse_target_argv`] or
//      [`baml_exec::parse_multi_target_argv`]) without running package
//      init — this gives `--help` / unknown-flag / missing-required
//      handling for free, with the same brand-purple styling as
//      `baml run`'s top-level clap.
//   4. Load `--json-args` and the `BAML_CONTEXT` launch context, then
//      initialize the BAML engine with the embedded program, the launch
//      context, and the argv shape the chosen mode expects.
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
    CONTEXT_ENV, DispatchResult, PACK_SECTION_NAME, PackEnvelope, PackMode, clamp_exit_code,
    context_source_from_env, dispatch_target, ensure_single_stdin_consumer, load_json_source,
    load_launch_context, parse_multi_target_argv, parse_target_argv, print_error,
};
use bex_engine::{BexEngine, UserFunctionCatalog, UserFunctionInfo};
use sys_native::SysOpsExt;

fn extract_envelope() -> Result<PackEnvelope, String> {
    let section = libsui::find_section(PACK_SECTION_NAME)
        .map_err(|e| format!("Failed to read embedded section: {e}"))?
        .ok_or("No embedded BAML package found. This binary must be built with `baml pack`.")?;

    let envelope: PackEnvelope =
        baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, section)
            .map_err(|e| format!("Failed to deserialize pack envelope: {e}"))?;
    envelope
        .verify_telemetry()
        .map_err(|error| error.to_string())?;
    Ok(envelope)
}

/// Build `baml.argv` per BEP-027 §"baml.argv in packaged binaries":
/// `argv[1]` is the target identifier, followed by the user's tokens.
fn build_argv(exe: &str, target_identifier: &str, tokens: &[String]) -> Vec<String> {
    let mut argv = Vec::with_capacity(2 + tokens.len());
    argv.push(exe.to_string());
    argv.push(target_identifier.to_string());
    argv.extend(tokens.iter().cloned());
    argv
}

/// The OS argv split into the executable name and the user's tokens.
fn os_args() -> (String, Vec<String>) {
    let mut os_args = std::env::args();
    let exe = os_args
        .next()
        .unwrap_or_else(|| "baml-pack-host".to_string());
    (exe, os_args.collect())
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

fn new_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    telemetry: btel_settings::artifact::ArtifactTelemetry,
    launch_context: btel_types::context::Context,
) -> Result<BexEngine, bex_engine::EngineError> {
    let recording = bex_engine::TelemetryRecording::from_artifact(&telemetry).with_host("pack");
    BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        bex_engine::EngineConfig {
            launch_context,
            runtime_compiler: Some(bex_project::runtime_compiler()),
            artifact_telemetry: Some(telemetry),
            recording: Some(recording),
            ..Default::default()
        },
    )
}

fn function_catalog(program: &bex_vm_types::Program) -> Option<UserFunctionCatalog> {
    UserFunctionCatalog::from_program(program)
        .inspect_err(|e| print_error(format_args!("failed to read packed program: {e}")))
        .ok()
}

/// Map a clap parse error to the process exit: help and version requests
/// succeed; everything else (including clap auto-showing help because the
/// invocation was invalid) fails so scripted callers can detect it.
fn clap_exit(err: &baml_exec::clap_reexport::Error) -> ExitCode {
    use baml_exec::clap_reexport::ErrorKind;
    let _ = err.print();
    match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
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
    let (exe, tokens) = os_args();

    let Some(catalog) = function_catalog(&envelope.program) else {
        return ExitCode::FAILURE;
    };
    let Some(func_info) = catalog.find_user_function(&target.qualified_name) else {
        print_error(format_args!(
            "function `{}` not found",
            target.qualified_name
        ));
        return ExitCode::FAILURE;
    };

    let parsed = if target_is_typed(&func_info) {
        match parse_target_argv(&exe, &target.qualified_name, &func_info, &tokens) {
            Ok(p) => p,
            Err(err) => return clap_exit(&err),
        }
    } else {
        baml_exec::ParsedTargetArgs::default()
    };

    let argv = build_argv(&exe, &target.subcommand_name, &tokens);
    let target_name = target.qualified_name.clone();
    launch(envelope, argv, &target_name, parsed)
}

/// Multi-subcommand dispatch: the binary acts like a multi-tool, with
/// one subcommand per packed function.
fn run_subcommand(envelope: PackEnvelope) -> ExitCode {
    let (exe, trailing) = os_args();

    let Some(catalog) = function_catalog(&envelope.program) else {
        return ExitCode::FAILURE;
    };
    // Per-target lookups for the clap subcommand builder.
    let mut lookups: HashMap<String, UserFunctionInfo> = HashMap::new();
    for entry in &envelope.targets {
        match catalog.find_user_function(&entry.qualified_name) {
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
            Err(err) => return clap_exit(&err),
        };

    // `argv[1]` is the resolved subcommand identifier so `baml.sys.argv()`
    // matches what the user typed (BEP-027 §"`baml.argv` in packaged
    // binaries"); the subcommand token itself stays in the tail.
    let chosen_entry = envelope
        .targets
        .iter()
        .find(|t| t.qualified_name == chosen)
        .expect("parse_multi_target_argv returned an unregistered target");
    let argv = build_argv(&exe, &chosen_entry.subcommand_name, &trailing);
    launch(envelope, argv, &chosen, parsed)
}

/// Load the call's inputs, then initialize the engine and dispatch. Inputs
/// load in the same order as `baml run`: stdin conflicts are rejected before
/// either source is read, and `--json-args` is read before the context.
fn launch(
    envelope: PackEnvelope,
    argv: Vec<String>,
    target_name: &str,
    parsed: baml_exec::ParsedTargetArgs,
) -> ExitCode {
    let context_source = match context_source_from_env() {
        Ok(source) => source,
        Err(e) => {
            print_error(format_args!("{e:#}"));
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = ensure_single_stdin_consumer(
        context_source.as_deref(),
        CONTEXT_ENV,
        parsed.json_source.as_deref(),
        "--json-args",
    ) {
        print_error(e);
        return ExitCode::FAILURE;
    }
    let json_args = match parsed
        .json_source
        .as_deref()
        .map(load_json_source)
        .transpose()
    {
        Ok(v) => v,
        Err(e) => {
            print_error(e);
            return ExitCode::FAILURE;
        }
    };
    let launch_context = match context_source
        .as_deref()
        .map(load_launch_context)
        .transpose()
    {
        Ok(context) => context.unwrap_or_default(),
        Err(e) => {
            print_error(format_args!("invalid {CONTEXT_ENV}: {e:#}"));
            return ExitCode::FAILURE;
        }
    };

    let engine = match new_engine(envelope.program, argv, envelope.telemetry, launch_context) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            print_error(format_args!("failed to initialize engine: {e}"));
            return ExitCode::FAILURE;
        }
    };
    finalize_dispatch(
        &engine,
        target_name,
        parsed.cli_values,
        json_args,
        envelope.output_format,
    )
}

fn finalize_dispatch(
    engine: &Arc<BexEngine>,
    target_name: &str,
    cli_values: HashMap<String, bex_engine::BexExternalValue>,
    json_args: Option<serde_json::Value>,
    output_format: baml_exec::OutputFormat,
) -> ExitCode {
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
        cli_values,
        json_args,
        output_format,
    ));
    engine.record_process_exit(match &result {
        Ok(DispatchResult::Ok | DispatchResult::Exit(0)) => bex_engine::ProcessStatus::Success,
        _ if engine.root_panicked() => bex_engine::ProcessStatus::Panicked,
        _ => bex_engine::ProcessStatus::Error,
    });
    rt.block_on(engine.shutdown());
    if let Some(error) = engine.initial_cloud_authorization_error() {
        print_error(format_args!("{error}"));
        return ExitCode::FAILURE;
    }
    if !engine.initial_telemetry_failure_handled()
        && let Some(Err(error)) = engine.telemetry_result()
    {
        eprintln!("Warning: telemetry recording failed: {error}");
    }
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
