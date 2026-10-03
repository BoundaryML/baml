//! Runtime recording configuration, separate from CLI usage analytics.
use std::{path::Path, sync::Arc};

use bex_engine::{BexEngine, EngineError, TelemetryRecording};
use sys_native::SysOpsExt;

/// Use the CLI's resolved project root, including on bytecode-cache hits.
/// Engine telemetry-off mode ignores the destination without creating files.
pub(crate) fn create_engine(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
    sources: Vec<(String, String)>,
) -> Result<BexEngine, EngineError> {
    create_engine_with_context(program, argv, project_root, sources, Default::default())
}

pub(crate) fn create_engine_with_context(
    program: bex_vm_types::Program,
    argv: Vec<String>,
    project_root: &Path,
    sources: Vec<(String, String)>,
    launch_context: btel_types::context::Context,
) -> Result<BexEngine, EngineError> {
    BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        bex_engine::EngineConfig {
            launch_context,
            runtime_compiler: Some(bex_project::runtime_compiler()),
            recording: Some(
                TelemetryRecording::from_boundary_env()
                    .unwrap_or_else(|| {
                        TelemetryRecording::local_files(
                            project_root,
                            btel_settings::publisher::RecordingConfig::default(),
                        )
                    })
                    .with_host("baml")
                    .with_sources(sources),
            ),
            ..bex_engine::EngineConfig::default()
        },
    )
}

/// The session's `.baml` sources keyed by path relative to the project root,
/// stored with each recording.
pub(crate) fn session_sources(
    session: &crate::project_session::ProjectSession,
) -> Vec<(String, String)> {
    let root = session.root();
    session
        .resolved
        .files
        .iter()
        .map(|(path, content)| {
            let path = path.strip_prefix(root).unwrap_or(path);
            (path.to_string_lossy().into_owned(), content.clone())
        })
        .collect()
}
