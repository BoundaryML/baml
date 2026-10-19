//! Runtime host for binaries produced by `baml pack`.
//!
//! The binary (`main.rs`) reads the packed program out of its own executable
//! and hands it to [`run_envelope`]. A natively compiled pack (`baml pack
//! --native`) is a generated crate that embeds the same packed program and
//! links the compiled bodies, and calls [`run_embedded`] with both.
//!
//! # `BAML_NATIVE_BODIES`
//!
//! A natively compiled binary runs its bytecode alone when the environment
//! variable `BAML_NATIVE_BODIES` is `0`, `off` or `false` (case-insensitive);
//! any other value, or none, keeps the linked bodies. This exists for
//! differential testing: the same binary, holding the same program bytes,
//! run twice with the same argv — once native, once interpreted — so the two
//! runs differ in nothing but which implementation of each function executed,
//! and their output and log format can be compared verbatim. A binary without
//! native bodies ignores the variable.
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
    DispatchResult, PackEnvelope, PackMode, clamp_exit_code, dispatch_target, load_json_source,
    parse_multi_target_argv, parse_target_argv, print_error,
};
use bex_engine::{BexEngine, UserFunctionInfo};
use sys_native::SysOpsExt;

/// The environment variable that turns a native binary's bodies off; see the
/// crate docs.
pub const NATIVE_BODIES_ENV: &str = "BAML_NATIVE_BODIES";

/// Whether a `BAML_NATIVE_BODIES` value asks for the bytecode alone.
fn native_bodies_disabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        value.eq_ignore_ascii_case("0")
            || value.eq_ignore_ascii_case("off")
            || value.eq_ignore_ascii_case("false")
    })
}

/// The bodies to run with: `native_bodies`, unless the environment turns
/// them off.
fn native_bodies_from_env(
    native_bodies: Option<bex_engine::NativeBodies>,
) -> Option<bex_engine::NativeBodies> {
    let value = std::env::var(NATIVE_BODIES_ENV).ok();
    if native_bodies_disabled(value.as_deref()) {
        None
    } else {
        native_bodies
    }
}

/// Run a decoded pack envelope: dispatch per its mode and exit.
///
/// `native_bodies` are the compiled bodies of a `baml pack --native` build,
/// attached to the program at load; `None` runs the bytecode alone, as does
/// `BAML_NATIVE_BODIES=0` in the environment (see the crate docs).
pub fn run_envelope(
    envelope: PackEnvelope,
    native_bodies: Option<bex_engine::NativeBodies>,
) -> ExitCode {
    let native_bodies = native_bodies_from_env(native_bodies);
    match envelope.mode {
        PackMode::Single => run_single(envelope, native_bodies),
        PackMode::Subcommand => run_subcommand(envelope, native_bodies),
    }
}

/// Decode and run a packed program embedded as bytes: the entry point of a
/// `baml pack --native` binary, whose generated crate holds the program the
/// bodies were compiled from.
pub fn run_embedded(
    artifact: &[u8],
    native_bodies: Option<&'static bex_engine::NativeBodyTable>,
) -> ExitCode {
    let envelope: Result<(PackEnvelope, [u8; 32]), String> =
        baml_artifact::decode_with_digest(baml_artifact::ArtifactKind::PackedProgram, artifact)
            .map_err(|e| format!("Failed to deserialize pack envelope: {e}"))
            .and_then(|(envelope, digest): (PackEnvelope, [u8; 32])| {
                envelope
                    .verify_telemetry()
                    .map_err(|error| error.to_string())?;
                Ok((envelope, digest))
            });
    match envelope {
        Ok((envelope, artifact_fingerprint)) => run_envelope(
            envelope,
            native_bodies.map(|table| bex_engine::NativeBodies {
                table,
                artifact_fingerprint,
            }),
        ),
        Err(e) => {
            print_error(e);
            ExitCode::FAILURE
        }
    }
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

/// The stack of a runtime worker thread: a main thread's default.
const WORKER_STACK: usize = 8 << 20;

/// Whether the target is typed — i.e. has one or more parameters whose
/// types drive the auto-CLI. Parameterless targets own their full argv
/// and bypass the clap layer entirely.
fn target_is_typed(info: &UserFunctionInfo) -> bool {
    !info.param_names.is_empty()
}

fn new_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    telemetry: btel_settings::artifact::ArtifactTelemetry,
    native_bodies: Option<bex_engine::NativeBodies>,
) -> Result<BexEngine, bex_engine::EngineError> {
    let recording = bex_engine::TelemetryRecording::from_artifact(&telemetry)?.with_host("pack");
    BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        bex_engine::EngineConfig {
            runtime_compiler: Some(bex_project::runtime_compiler()),
            artifact_telemetry: Some(telemetry),
            recording: Some(recording),
            native_bodies,
            ..Default::default()
        },
    )
}

/// Single-target dispatch: the binary acts like a one-shot CLI; flags on
/// the binary bind directly to the target's parameters.
fn run_single(envelope: PackEnvelope, native_bodies: Option<bex_engine::NativeBodies>) -> ExitCode {
    debug_assert_eq!(
        envelope.targets.len(),
        1,
        "single mode = exactly one target"
    );
    let target = &envelope.targets[0];

    let argv = build_argv(&target.subcommand_name);

    let engine = match new_engine(
        envelope.program,
        argv.clone(),
        envelope.telemetry,
        native_bodies,
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
fn run_subcommand(
    envelope: PackEnvelope,
    native_bodies: Option<bex_engine::NativeBodies>,
) -> ExitCode {
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

    let mut engine = match new_engine(
        envelope.program,
        bootstrap_argv,
        envelope.telemetry,
        native_bodies,
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

    // Spawned tasks run on the workers. Give them the main thread's stack, so
    // a task's compiled code recurses as deep as the root call's before the
    // VM moves it to a fresh stack segment (deep compiled recursion reaches
    // the frame limit on any stack; the first segment is what this sizes);
    // only touched pages cost memory.
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(WORKER_STACK)
        .build()
    {
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

#[cfg(test)]
mod tests {
    use super::native_bodies_disabled;

    /// Only an explicit off value turns the bodies off: an unset or
    /// unrecognized variable keeps the binary's default.
    #[test]
    fn native_bodies_env_off_values() {
        for value in ["0", "off", "OFF", "Off", "false", "FALSE", " 0 ", "False"] {
            assert!(native_bodies_disabled(Some(value)), "{value:?}");
        }
        for value in ["1", "on", "true", "", "yes", "no", "native"] {
            assert!(!native_bodies_disabled(Some(value)), "{value:?}");
        }
        assert!(!native_bodies_disabled(None));
    }
}
