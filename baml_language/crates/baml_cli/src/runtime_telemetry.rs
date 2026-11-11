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
    let boundary = crate::cloud_config::Boundary::read(project_root)
        .map_err(|error| EngineError::Other(error.to_string()))?;
    BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        argv,
        bex_engine::EngineConfig {
            launch_context,
            runtime_compiler: Some(bex_project::runtime_compiler()),
            recording: Some(
                TelemetryRecording::from_boundary_defaults(
                    boundary.project.as_deref(),
                    boundary.api_url.as_deref(),
                )
                .unwrap_or_else(|| {
                    TelemetryRecording::local_files(
                        project_root,
                        btel_settings::publisher::RecordingConfig::default(),
                    )
                })
                .with_host("baml")
                .with_sources(sources)
                .with_source_upload(boundary.upload_sources.unwrap_or(true)),
            ),
            ..bex_engine::EngineConfig::default()
        },
    )
}

/// The session's sources keyed by path relative to the project root, stored
/// with each recording: its `.baml` files, and its `baml.toml` when it has
/// one. The manifest names the package, so a reader of several programs'
/// recordings can tell which program a source revision is of, and it is part
/// of what a revision changed.
pub(crate) fn session_sources(
    session: &crate::project_session::ProjectSession,
) -> Vec<(String, String)> {
    let root = session.root();
    let manifest = session
        .resolved
        .manifest
        .iter()
        .map(|manifest| (baml_db::BAML_TOML.to_owned(), manifest.clone()));
    let files = session.resolved.files.iter().map(|(path, content)| {
        let path = path.strip_prefix(root).unwrap_or(path);
        (path.to_string_lossy().into_owned(), content.clone())
    });
    manifest.chain(files).collect()
}
