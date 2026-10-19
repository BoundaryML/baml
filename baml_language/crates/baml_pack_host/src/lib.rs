//! Runtime host for binaries produced by `baml pack`.

// `baml.sys.exit` must terminate the process with the target's exit code.
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

    let envelope: PackEnvelope =
        baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, section)
            .map_err(|e| format!("Failed to deserialize pack envelope: {e}"))?;
    envelope
        .verify_telemetry()
        .map_err(|error| error.to_string())?;
    Ok(envelope)
}

fn compiled_bodies_disabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        value.eq_ignore_ascii_case("0")
            || value.eq_ignore_ascii_case("off")
            || value.eq_ignore_ascii_case("false")
    })
}

fn compiled_bodies_from_env(
    compiled_bodies: Option<bex_engine::CompiledBodies>,
) -> Option<bex_engine::CompiledBodies> {
    let value = std::env::var("BAML_COMPILED_BODIES").ok();
    if compiled_bodies_disabled(value.as_deref()) {
        None
    } else {
        compiled_bodies
    }
}

fn run_envelope(
    envelope: PackEnvelope,
    compiled_bodies: Option<bex_engine::CompiledBodies>,
) -> ExitCode {
    let compiled_bodies = compiled_bodies_from_env(compiled_bodies);
    match envelope.mode {
        PackMode::Single => run_single(envelope, compiled_bodies),
        PackMode::Subcommand => run_subcommand(envelope, compiled_bodies),
    }
}

/// Compiled bodies must match the authenticated artifact fingerprint.
pub fn run_embedded(
    artifact: &[u8],
    compiled_bodies: Option<&'static bex_engine::CompiledBodyTable>,
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
            compiled_bodies.map(|table| bex_engine::CompiledBodies {
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

// BEP-027 requires the target identifier at `argv[1]`.
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

const WORKER_STACK: usize = 8 << 20;

// Parameterless targets parse their own argv.
fn target_is_typed(info: &UserFunctionInfo) -> bool {
    !info.param_names.is_empty()
}

pub fn main() -> ExitCode {
    match extract_envelope() {
        Ok(envelope) => run_envelope(envelope, None),
        Err(e) => {
            print_error(e);
            ExitCode::FAILURE
        }
    }
}

fn new_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    telemetry: btel_settings::artifact::ArtifactTelemetry,
    compiled_bodies: Option<bex_engine::CompiledBodies>,
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
            compiled_bodies,
            ..Default::default()
        },
    )
}

fn run_single(
    envelope: PackEnvelope,
    compiled_bodies: Option<bex_engine::CompiledBodies>,
) -> ExitCode {
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
        compiled_bodies,
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

fn run_subcommand(
    envelope: PackEnvelope,
    compiled_bodies: Option<bex_engine::CompiledBodies>,
) -> ExitCode {
    let mut os_args = std::env::args();
    let exe = os_args
        .next()
        .unwrap_or_else(|| "baml-pack-host".to_string());
    let trailing: Vec<String> = os_args.collect();

    // Initialize argv before the subcommand is known, then patch `argv[1]`
    // while the engine is still uniquely owned.
    let mut bootstrap_argv = Vec::with_capacity(2 + trailing.len());
    bootstrap_argv.push(exe.clone());
    bootstrap_argv.push(String::new());
    bootstrap_argv.extend(trailing.iter().cloned());

    let mut engine = match new_engine(
        envelope.program,
        bootstrap_argv,
        envelope.telemetry,
        compiled_bodies,
    ) {
        Ok(e) => e,
        Err(e) => {
            print_error(format_args!("failed to initialize engine: {e}"));
            return ExitCode::FAILURE;
        }
    };

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
                // Help shown for missing arguments must still exit non-zero.
                return match kind {
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                    _ => ExitCode::FAILURE,
                };
            }
        };

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

    // Compiled recursion needs an 8 MiB stack on worker threads.
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
    use super::compiled_bodies_disabled;

    #[test]
    fn compiled_bodies_env_off_values() {
        for value in ["0", "off", "OFF", "Off", "false", "FALSE", " 0 ", "False"] {
            assert!(compiled_bodies_disabled(Some(value)), "{value:?}");
        }
        for value in ["1", "on", "true", "", "yes", "no", "rust"] {
            assert!(!compiled_bodies_disabled(Some(value)), "{value:?}");
        }
        assert!(!compiled_bodies_disabled(None));
    }
}
